//! Explicit values, blocks and edge parameters. Verification precedes emission.
use super::{
    hir::{
        arithmetic_type, Arithmetic, ArrayOperation, BindingId, Bitwise, Comparison, Expression,
        Hir, Literal, LoopId, Nominal, Type,
    },
    resolve::Global,
    Diagnostic,
};
use std::{
    collections::{HashMap, HashSet},
    ops::Range,
};
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ValueId(pub usize);
#[derive(Debug, Clone)]
pub struct Value {
    pub ty: Type,
    pub span: Range<usize>,
}
#[derive(Debug, Clone)]
pub enum Operation {
    Bitwise {
        operation: Bitwise,
        arguments: Vec<ValueId>,
    },
    Comparison {
        operation: Comparison,
        arguments: Vec<ValueId>,
    },
    Array {
        operation: ArrayOperation,
        arguments: Vec<ValueId>,
    },
    /// Nil or internal undefined, applied to one already evaluated value.
    NilTest(ValueId),
    /// Snapshot operands, initializer operands and compiled body, in that order.
    DynamicScope {
        globals: Vec<Global>,
        operands: Vec<ValueId>,
    },
    /// Evaluated compiled regions, in body/handler/cleanup order.
    Try {
        regions: [ValueId; 3],
    },
    Nominal {
        operation: Nominal,
        arguments: Vec<ValueId>,
    },
    Literal(Literal),
    GlobalRead(Global),
    GlobalCell(Global),
    GlobalBound(Global),
    /// Publish only an already evaluated value; result is that same dynamic value.
    GlobalWrite {
        global: Global,
        value: ValueId,
    },
    MakeClosure {
        body: Box<ClosureBody>,
        captures: Vec<ValueId>,
    },
    MakeGeneralClosure {
        body: Box<GeneralClosureBody>,
        captures: Vec<ValueId>,
    },
    /// Callee is the first operand; all arguments are already evaluated values.
    Call {
        operands: Vec<ValueId>,
    },
    Arithmetic {
        operator: Arithmetic,
        arguments: Vec<ValueId>,
    },
}
#[derive(Debug, Clone)]
pub struct Instruction {
    pub result: ValueId,
    pub operation: Operation,
    pub span: Range<usize>,
}
#[derive(Debug, Clone)]
pub enum Terminator {
    Throw(ValueId),
    Return(ValueId),
    Jump {
        target: usize,
        arguments: Vec<ValueId>,
    },
    Branch {
        condition: ValueId,
        consequent: usize,
        alternative: usize,
    },
}
#[derive(Debug, Clone)]
pub struct Block {
    pub parameters: Vec<ValueId>,
    pub instructions: Vec<Instruction>,
    pub terminator: Terminator,
}
#[derive(Debug, Clone)]
pub struct Function {
    pub values: Vec<Value>,
    pub blocks: Vec<Block>,
    pub span: Range<usize>,
}
#[derive(Debug, Clone)]
pub struct ClosureBody {
    pub capture_types: Vec<Type>,
    pub arity: usize,
    pub function: Function,
}
#[derive(Debug, Clone)]
pub struct GeneralClosureBody {
    pub capture_types: Vec<Type>,
    pub methods: Vec<ClosureBody>,
    pub self_capture: bool,
}
struct Lowerer {
    function: Function,
    current: usize,
    bindings: HashMap<BindingId, ValueId>,
    targets: HashMap<LoopId, (usize, usize)>,
}
impl Lowerer {
    fn new(span: Range<usize>) -> Self {
        let mut lowerer = Self {
            function: Function {
                values: Vec::new(),
                blocks: Vec::new(),
                span,
            },
            current: 0,
            bindings: HashMap::new(),
            targets: HashMap::new(),
        };
        lowerer.block(Vec::new());
        lowerer
    }
    fn parameter(&mut self, id: BindingId, ty: Type, span: Range<usize>) -> Result<(), Diagnostic> {
        let value = self.value(ty, span.clone());
        if self.bindings.insert(id, value).is_some() {
            return Err(Diagnostic {
                span,
                message: "HIR capture/parameter identity defined twice".into(),
            });
        }
        self.function.blocks[0].parameters.push(value);
        Ok(())
    }
    fn finish(mut self, hir: &Hir) -> Result<Function, Diagnostic> {
        let value = self.expression(hir)?;
        if let Some(value) = value {
            self.function.blocks[self.current].terminator = Terminator::Return(value);
        }
        Ok(self.function)
    }
    fn value(&mut self, ty: Type, span: Range<usize>) -> ValueId {
        let id = ValueId(self.function.values.len());
        self.function.values.push(Value { ty, span });
        id
    }
    fn block(&mut self, parameters: Vec<ValueId>) -> usize {
        let id = self.function.blocks.len();
        self.function.blocks.push(Block {
            parameters,
            instructions: Vec::new(),
            terminator: Terminator::Return(ValueId(usize::MAX)),
        });
        id
    }
    fn emit(&mut self, operation: Operation, ty: Type, span: Range<usize>) -> ValueId {
        let result = self.value(ty, span.clone());
        self.function.blocks[self.current]
            .instructions
            .push(Instruction {
                result,
                operation,
                span,
            });
        result
    }
    fn literal(&mut self, value: Literal, span: Range<usize>) -> ValueId {
        let ty = value.ty();
        self.emit(Operation::Literal(value), ty, span)
    }
    fn expression(&mut self, hir: &Hir) -> Result<Option<ValueId>, Diagnostic> {
        // Divergent operands stop lowering following effects, including publication.
        macro_rules! operand {
            ($value:expr) => {
                match self.expression($value)? {
                    Some(value) => value,
                    None => return Ok(None),
                }
            };
        }

        Ok(Some(match &hir.kind {
            Expression::Assign { global, value } => {
                let value = operand!(value);
                self.emit(
                    Operation::GlobalWrite {
                        global: global.clone(),
                        value,
                    },
                    Type::Value,
                    hir.span.clone(),
                )
            }
            Expression::DynamicScope { bindings, body } => {
                let globals = bindings
                    .iter()
                    .map(|(global, _)| global.clone())
                    .collect::<Vec<_>>();
                let mut operands = globals
                    .iter()
                    .map(|global| {
                        self.emit(
                            Operation::GlobalRead(global.clone()),
                            Type::Value,
                            hir.span.clone(),
                        )
                    })
                    .collect::<Vec<_>>();
                for (_, value) in bindings {
                    operands.push(operand!(value));
                }
                operands.push(operand!(body));
                self.emit(
                    Operation::DynamicScope { globals, operands },
                    Type::Value,
                    hir.span.clone(),
                )
            }
            Expression::Throw(value) => {
                let value = operand!(value);
                self.function.blocks[self.current].terminator = Terminator::Throw(value);
                return Ok(None);
            }
            Expression::Try { regions } => {
                let regions = [
                    operand!(&regions[0]),
                    operand!(&regions[1]),
                    operand!(&regions[2]),
                ];
                self.emit(Operation::Try { regions }, Type::Value, hir.span.clone())
            }
            Expression::Literal(value) => self.literal(value.clone(), hir.span.clone()),
            Expression::GlobalCell(global) => {
                if hir.ty != Type::Value {
                    return Err(Diagnostic {
                        span: hir.span.clone(),
                        message: "Invalid cell-reference HIR type".into(),
                    });
                }
                self.emit(
                    Operation::GlobalCell(global.clone()),
                    Type::Value,
                    hir.span.clone(),
                )
            }
            Expression::Global(global) => self.emit(
                Operation::GlobalRead(global.clone()),
                Type::Value,
                hir.span.clone(),
            ),
            Expression::Definition {
                global,
                initializer,
                once,
                ..
            } => {
                if let Some(initializer) = initializer {
                    if *once {
                        let condition = self.emit(
                            Operation::GlobalBound(global.clone()),
                            Type::Bool,
                            hir.span.clone(),
                        );
                        let result = self.value(Type::Value, hir.span.clone());
                        let bound = self.block(Vec::new());
                        let unbound = self.block(Vec::new());
                        let join = self.block(vec![result]);
                        self.function.blocks[self.current].terminator = Terminator::Branch {
                            condition,
                            consequent: bound,
                            alternative: unbound,
                        };
                        self.current = bound;
                        let nil = self.literal(Literal::Nil, hir.span.clone());
                        self.function.blocks[self.current].terminator = Terminator::Jump {
                            target: join,
                            arguments: vec![nil],
                        };
                        self.current = unbound;
                        if let Some(value) = self.expression(initializer)? {
                            let value = self.emit(
                                Operation::GlobalWrite {
                                    global: global.clone(),
                                    value,
                                },
                                Type::Value,
                                hir.span.clone(),
                            );
                            self.function.blocks[self.current].terminator = Terminator::Jump {
                                target: join,
                                arguments: vec![value],
                            };
                        }
                        self.current = join;
                        result
                    } else {
                        let value = operand!(initializer);
                        self.emit(
                            Operation::GlobalWrite {
                                global: global.clone(),
                                value,
                            },
                            Type::Value,
                            hir.span.clone(),
                        )
                    }
                } else {
                    // A declaration does not reset an existing cell or bind an unbound one.
                    if *once {
                        return Err(Diagnostic {
                            span: hir.span.clone(),
                            message: "HIR defonce requires initializer".into(),
                        });
                    }
                    self.literal(Literal::Nil, hir.span.clone())
                }
            }
            Expression::Local(id) => *self.bindings.get(id).ok_or_else(|| Diagnostic {
                span: hir.span.clone(),
                message: "HIR references an undefined binding".into(),
            })?,
            Expression::Call { callee, arguments } => {
                let mut operands = vec![operand!(callee)];
                for argument in arguments {
                    operands.push(operand!(argument));
                }
                self.emit(Operation::Call { operands }, Type::Value, hir.span.clone())
            }
            Expression::Function {
                parameters,
                captures,
                body,
            } => {
                let mut captured = Vec::new();
                let mut capture_types = Vec::new();
                let mut inner = Lowerer::new(body.span.clone());
                for id in captures {
                    let value = *self.bindings.get(id).ok_or_else(|| Diagnostic {
                        span: hir.span.clone(),
                        message: "Undefined HIR capture".into(),
                    })?;
                    let ty = self.function.values[value.0].ty;
                    captured.push(value);
                    capture_types.push(ty);
                    inner.parameter(*id, ty, hir.span.clone())?;
                }
                for parameter in parameters {
                    inner.parameter(parameter.id, Type::Value, parameter.span.clone())?;
                }
                let function = inner.finish(body)?;
                self.emit(
                    Operation::MakeClosure {
                        body: Box::new(ClosureBody {
                            capture_types,
                            arity: parameters.len(),
                            function,
                        }),
                        captures: captured,
                    },
                    Type::Closure(parameters.len()),
                    hir.span.clone(),
                )
            }
            Expression::GeneralFunction {
                methods,
                captures,
                self_binding,
            } => {
                let mut captured = Vec::new();
                let mut capture_types = Vec::new();
                for id in captures {
                    let value = *self.bindings.get(id).ok_or_else(|| Diagnostic {
                        span: hir.span.clone(),
                        message: "Undefined HIR capture".into(),
                    })?;
                    captured.push(value);
                    capture_types.push(self.function.values[value.0].ty);
                }
                let mut lowered = Vec::new();
                for method in methods {
                    let mut inner = Lowerer::new(method.body.span.clone());
                    for (id, ty) in captures.iter().zip(&capture_types) {
                        inner.parameter(*id, *ty, hir.span.clone())?;
                    }
                    let mut environment_types = capture_types.clone();
                    if let Some(parameter) = self_binding {
                        inner.parameter(parameter.id, Type::Value, parameter.span.clone())?;
                        environment_types.push(Type::Value);
                    }
                    for parameter in &method.parameters {
                        inner.parameter(parameter.id, Type::Value, parameter.span.clone())?;
                    }
                    lowered.push(ClosureBody {
                        capture_types: environment_types,
                        arity: method.parameters.len(),
                        function: inner.finish(&method.body)?,
                    });
                }
                self.emit(
                    Operation::MakeGeneralClosure {
                        body: Box::new(GeneralClosureBody {
                            capture_types,
                            methods: lowered,
                            self_capture: self_binding.is_some(),
                        }),
                        captures: captured,
                    },
                    Type::Value,
                    hir.span.clone(),
                )
            }
            Expression::Do(items) => {
                if items.is_empty() {
                    self.literal(Literal::Nil, hir.span.clone())
                } else {
                    for item in &items[..items.len() - 1] {
                        operand!(item);
                    }
                    return self.expression(items.last().unwrap());
                }
            }
            Expression::Loop {
                target,
                bindings,
                body,
            } => {
                if self.targets.contains_key(target) {
                    return Err(Diagnostic {
                        span: hir.span.clone(),
                        message: "HIR recurrence target defined twice".into(),
                    });
                }
                let mut initial = Vec::new();
                let mut parameters = Vec::new();
                for binding in bindings {
                    let value = operand!(&binding.value);
                    if self.bindings.insert(binding.id, value).is_some() {
                        return Err(Diagnostic {
                            span: binding.span.clone(),
                            message: "HIR binding identity is defined twice".into(),
                        });
                    }
                    initial.push(value);
                }
                for binding in bindings {
                    parameters.push(self.value(Type::Value, binding.span.clone()));
                }
                let header = self.block(parameters.clone());
                self.function.blocks[self.current].terminator = Terminator::Jump {
                    target: header,
                    arguments: initial,
                };
                for (binding, value) in bindings.iter().zip(parameters) {
                    self.bindings.insert(binding.id, value);
                }
                self.targets.insert(*target, (header, bindings.len()));
                self.current = header;
                let result = self.expression(body);
                self.targets.remove(target);
                return result;
            }
            Expression::Recur { target, arguments } => {
                let &(header, arity) = self.targets.get(target).ok_or_else(|| Diagnostic {
                    span: hir.span.clone(),
                    message: "HIR recurrence target is undefined".into(),
                })?;
                if arguments.len() != arity {
                    return Err(Diagnostic {
                        span: hir.span.clone(),
                        message: "HIR recurrence arity mismatch".into(),
                    });
                }
                let mut values = Vec::new();
                for argument in arguments {
                    values.push(operand!(argument));
                }
                self.function.blocks[self.current].terminator = Terminator::Jump {
                    target: header,
                    arguments: values,
                };
                return Ok(None);
            }
            Expression::Let { bindings, body } => {
                // Binding identities, rather than names, distinguish shadowed values.
                for binding in bindings {
                    let value = operand!(&binding.value);
                    if self.bindings.insert(binding.id, value).is_some() {
                        return Err(Diagnostic {
                            span: binding.span.clone(),
                            message: "HIR binding identity is defined twice".into(),
                        });
                    }
                }
                return self.expression(body);
            }
            Expression::If {
                condition,
                consequent,
                alternative,
            } => {
                let condition = operand!(condition);
                let then_block = self.block(Vec::new());
                let else_block = self.block(Vec::new());
                self.function.blocks[self.current].terminator = Terminator::Branch {
                    condition,
                    consequent: then_block,
                    alternative: else_block,
                };
                self.current = then_block;
                let then_value = self.expression(consequent)?;
                let then_end = self.current;
                self.current = else_block;
                let else_value = self.expression(alternative)?;
                let else_end = self.current;
                if then_value.is_none() && else_value.is_none() {
                    return Ok(None);
                }
                let result = self.value(hir.ty, hir.span.clone());
                let join = self.block(vec![result]);
                for (end, value) in [(then_end, then_value), (else_end, else_value)] {
                    if let Some(value) = value {
                        self.function.blocks[end].terminator = Terminator::Jump {
                            target: join,
                            arguments: vec![value],
                        };
                    }
                }
                self.current = join;
                result
            }
            Expression::NilTest(value) => {
                if hir.ty != Type::Bool {
                    return Err(Diagnostic {
                        span: hir.span.clone(),
                        message: "Invalid nil-test HIR result type".into(),
                    });
                }
                let value = operand!(value);
                self.emit(Operation::NilTest(value), Type::Bool, hir.span.clone())
            }
            Expression::Bitwise {
                operation,
                arguments,
            } => {
                if arguments.len() != operation.arity() || hir.ty != operation.result() {
                    return Err(Diagnostic {
                        span: hir.span.clone(),
                        message: "Invalid bitwise HIR shape".into(),
                    });
                }
                let mut values = Vec::new();
                for argument in arguments {
                    values.push(operand!(argument));
                }
                self.emit(
                    Operation::Bitwise {
                        operation: *operation,
                        arguments: values,
                    },
                    operation.result(),
                    hir.span.clone(),
                )
            }
            Expression::Comparison {
                operation,
                arguments,
            } => {
                if arguments.len() != 2 || hir.ty != Type::Bool {
                    return Err(Diagnostic {
                        span: hir.span.clone(),
                        message: "Invalid comparison HIR shape".into(),
                    });
                }
                let mut values = Vec::new();
                for argument in arguments {
                    values.push(operand!(argument));
                }
                self.emit(
                    Operation::Comparison {
                        operation: *operation,
                        arguments: values,
                    },
                    Type::Bool,
                    hir.span.clone(),
                )
            }
            Expression::Array {
                operation,
                arguments,
            } => {
                let types = arguments
                    .iter()
                    .map(|argument| argument.ty)
                    .collect::<Vec<_>>();
                if !operation.valid(types.len()) || hir.ty != Type::Value {
                    return Err(Diagnostic {
                        span: hir.span.clone(),
                        message: "Invalid array HIR shape".into(),
                    });
                }
                let mut values = Vec::new();
                for argument in arguments {
                    values.push(operand!(argument));
                }
                let arguments = values;
                self.emit(
                    Operation::Array {
                        operation: *operation,
                        arguments,
                    },
                    hir.ty,
                    hir.span.clone(),
                )
            }
            Expression::Nominal {
                operation,
                arguments,
            } => {
                let types = arguments
                    .iter()
                    .map(|argument| argument.ty)
                    .collect::<Vec<_>>();
                if !operation.valid(&types) || hir.ty != operation.result() {
                    return Err(Diagnostic {
                        span: hir.span.clone(),
                        message: "Invalid nominal HIR shape".into(),
                    });
                }
                let mut values = Vec::new();
                for argument in arguments {
                    values.push(operand!(argument));
                }
                let arguments = values;
                self.emit(
                    Operation::Nominal {
                        operation: *operation,
                        arguments,
                    },
                    hir.ty,
                    hir.span.clone(),
                )
            }
            Expression::Arithmetic {
                operator,
                arguments,
            } => {
                // Public HIR can represent Negate directly or malformed call arities.
                if (*operator == Arithmetic::Negate && arguments.len() != 1)
                    || (matches!(operator, Arithmetic::Subtract | Arithmetic::Divide)
                        && arguments.is_empty())
                {
                    return Err(Diagnostic {
                        span: hir.span.clone(),
                        message: "HIR arithmetic arity mismatch".into(),
                    });
                }
                // Evaluate every operand before entering the arithmetic operation.
                let mut values = Vec::new();
                for argument in arguments {
                    values.push(operand!(argument));
                }
                if values.is_empty() {
                    return Ok(Some(self.literal(
                        Literal::Number(if *operator == Arithmetic::Add {
                            0.0
                        } else {
                            1.0
                        }),
                        hir.span.clone(),
                    )));
                }
                let mut result = values[0];
                if values.len() == 1 {
                    if matches!(operator, Arithmetic::Subtract | Arithmetic::Negate) {
                        return Ok(Some(self.emit(
                            Operation::Arithmetic {
                                operator: Arithmetic::Negate,
                                arguments: values,
                            },
                            Type::Number,
                            hir.span.clone(),
                        )));
                    }
                    if *operator == Arithmetic::Divide {
                        let one = self.literal(Literal::Number(1.0), hir.span.clone());
                        return Ok(Some(self.emit(
                            Operation::Arithmetic {
                                operator: *operator,
                                arguments: vec![one, result],
                            },
                            Type::Number,
                            hir.span.clone(),
                        )));
                    }
                }
                for value in &values[1..] {
                    let ty = arithmetic_type(
                        *operator,
                        &[
                            self.function.values[result.0].ty,
                            self.function.values[value.0].ty,
                        ],
                    )
                    .ok_or_else(|| Diagnostic {
                        span: hir.span.clone(),
                        message: "HIR unsupported arithmetic operands".into(),
                    })?;
                    result = self.emit(
                        Operation::Arithmetic {
                            operator: *operator,
                            arguments: vec![result, *value],
                        },
                        ty,
                        hir.span.clone(),
                    );
                }
                result
            }
        }))
    }
}
/// Public HIR callers must obey the same lexical target and tail contract as source.
fn verify_recurrence(
    hir: &Hir,
    targets: &mut Vec<(LoopId, usize)>,
    tail: bool,
) -> Result<(), Diagnostic> {
    let error = |message: &str| Diagnostic {
        span: hir.span.clone(),
        message: message.into(),
    };
    match &hir.kind {
        Expression::Assign { value, .. } => verify_recurrence(value, targets, false)?,
        Expression::DynamicScope { bindings, body } => {
            if hir.ty != Type::Value
                || !matches!(&body.kind, Expression::Function { parameters, .. } if parameters.is_empty())
            {
                return Err(error("HIR dynamic scope shape mismatch"));
            }
            for (_, value) in bindings {
                verify_recurrence(value, targets, false)?;
            }
            verify_recurrence(body, &mut Vec::new(), false)?;
        }
        Expression::Throw(value) => verify_recurrence(value, targets, false)?,
        Expression::Try { regions } => {
            if hir.ty != Type::Value {
                return Err(error("HIR handler result must be dynamic Value"));
            }
            for (index, region) in regions.iter().enumerate() {
                let arity = usize::from(index == 1);
                if !matches!(&region.kind, Expression::Function { parameters, .. } if parameters.len() == arity)
                    && !(index > 0 && matches!(&region.kind, Expression::Literal(Literal::Nil)))
                {
                    return Err(error("HIR handler region shape mismatch"));
                }
                verify_recurrence(region, &mut Vec::new(), false)?;
            }
        }
        Expression::Loop {
            target,
            bindings,
            body,
        } => {
            if targets.iter().any(|(id, _)| id == target) {
                return Err(error("HIR recurrence target defined twice"));
            }
            for binding in bindings {
                verify_recurrence(&binding.value, targets, false)?;
            }
            targets.push((*target, bindings.len()));
            let result = verify_recurrence(body, targets, true);
            targets.pop();
            result?;
        }
        Expression::Recur { target, arguments } => {
            if !tail {
                return Err(error("HIR recurrence must be in tail position"));
            }
            if targets.last() != Some(&(*target, arguments.len())) {
                return Err(error("HIR recurrence target or arity mismatch"));
            }
            // Only the innermost target is legal: inner loop cannot recur an outer loop.
            for argument in arguments {
                verify_recurrence(argument, targets, false)?;
            }
        }
        Expression::Function { body, .. } => verify_recurrence(body, &mut Vec::new(), true)?,
        Expression::GeneralFunction { methods, .. } => {
            if methods.is_empty() {
                return Err(error("HIR function requires a signature"));
            }
            for method in methods {
                verify_recurrence(&method.body, &mut Vec::new(), true)?;
            }
        }
        Expression::Let { bindings, body } => {
            for binding in bindings {
                verify_recurrence(&binding.value, targets, false)?;
            }
            verify_recurrence(body, targets, tail)?;
        }
        Expression::Do(items) => {
            for (index, item) in items.iter().enumerate() {
                verify_recurrence(item, targets, tail && index + 1 == items.len())?;
            }
        }
        Expression::If {
            condition,
            consequent,
            alternative,
        } => {
            verify_recurrence(condition, targets, false)?;
            verify_recurrence(consequent, targets, tail)?;
            verify_recurrence(alternative, targets, tail)?;
        }
        Expression::Call { callee, arguments } => {
            verify_recurrence(callee, targets, false)?;
            for argument in arguments {
                verify_recurrence(argument, targets, false)?;
            }
        }
        Expression::NilTest(value) => verify_recurrence(value, targets, false)?,
        Expression::Arithmetic { arguments, .. }
        | Expression::Bitwise { arguments, .. }
        | Expression::Comparison { arguments, .. }
        | Expression::Array { arguments, .. }
        | Expression::Nominal { arguments, .. } => {
            for argument in arguments {
                verify_recurrence(argument, targets, false)?;
            }
        }
        Expression::Definition {
            initializer: Some(value),
            ..
        } => verify_recurrence(value, targets, false)?,
        _ => {}
    }
    Ok(())
}

pub fn lower(hir: &Hir) -> Result<Function, Diagnostic> {
    verify_recurrence(hir, &mut Vec::new(), false)?;
    Lowerer::new(hir.span.clone()).finish(hir)
}

fn operands(operation: &Operation) -> &[ValueId] {
    match operation {
        Operation::Literal(_)
        | Operation::GlobalRead(_)
        | Operation::GlobalCell(_)
        | Operation::GlobalBound(_) => &[],
        Operation::GlobalWrite { value, .. } | Operation::NilTest(value) => {
            std::slice::from_ref(value)
        }
        Operation::Arithmetic { arguments, .. }
        | Operation::Bitwise { arguments, .. }
        | Operation::Comparison { arguments, .. }
        | Operation::Array { arguments, .. }
        | Operation::Nominal { arguments, .. } => arguments,
        Operation::MakeClosure { captures, .. }
        | Operation::MakeGeneralClosure { captures, .. } => captures,
        Operation::Call { operands } | Operation::DynamicScope { operands, .. } => operands,
        Operation::Try { regions } => regions,
    }
}
/// Check graph integrity, unique definitions, dominance, edge arities and types.
/// Calls have ordered operands, verified closure/capture shape and known arities.
/// General exception/effect/suspension and tail-position analysis remain incomplete.
pub fn verify(function: &Function) -> Result<(), Diagnostic> {
    verify_function(function, &[], 0)
}
fn verify_function(
    function: &Function,
    entry_types: &[Type],
    depth: usize,
) -> Result<(), Diagnostic> {
    let fail = |message: &str| Diagnostic {
        span: function.span.clone(),
        message: message.into(),
    };
    let n = function.blocks.len();
    if depth > 64 {
        return Err(fail("Closure nesting exceeds 64"));
    }
    if n == 0 || function.blocks[0].parameters.len() != entry_types.len() {
        return Err(fail("IR entry parameter shape mismatch"));
    }
    for (id, expected) in function.blocks[0].parameters.iter().zip(entry_types) {
        if !function
            .values
            .get(id.0)
            .is_some_and(|value| value.ty == *expected)
        {
            return Err(fail("IR entry parameter type mismatch"));
        }
    }
    let mut definitions = vec![None; function.values.len()];
    let mut predecessors = vec![Vec::new(); n];
    let mut successors = vec![Vec::new(); n];
    for (block_id, block) in function.blocks.iter().enumerate() {
        for (position, value) in block
            .parameters
            .iter()
            .chain(block.instructions.iter().map(|inst| &inst.result))
            .enumerate()
        {
            let Some(definition) = definitions.get_mut(value.0) else {
                return Err(fail("IR definition is out of range"));
            };
            if definition.replace((block_id, position)).is_some() {
                return Err(fail("IR value is defined twice"));
            }
        }
        let targets = match &block.terminator {
            Terminator::Return(_) | Terminator::Throw(_) => Vec::new(),
            Terminator::Jump { target, .. } => vec![*target],
            Terminator::Branch {
                consequent,
                alternative,
                ..
            } => vec![*consequent, *alternative],
        };
        for target in targets {
            if target >= n {
                return Err(fail("IR edge is out of range"));
            }
            predecessors[target].push(block_id);
            successors[block_id].push(target);
        }
    }
    if definitions.iter().any(Option::is_none) {
        return Err(fail("IR value has no definition"));
    }
    let mut reachable = HashSet::new();
    let mut work = vec![0];
    while let Some(id) = work.pop() {
        if reachable.insert(id) {
            work.extend(&successors[id]);
        }
    }
    if reachable.len() != n {
        return Err(fail("IR contains unreachable blocks"));
    }
    let mut dominators = vec![reachable.clone(); n];
    dominators[0] = HashSet::from([0]);
    loop {
        let mut changed = false;
        for block in 1..n {
            let mut next = reachable.clone();
            for pred in &predecessors[block] {
                next.retain(|id| dominators[*pred].contains(id));
            }
            next.insert(block);
            if next != dominators[block] {
                dominators[block] = next;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    let ty = |value: ValueId| {
        function
            .values
            .get(value.0)
            .map(|value| value.ty)
            .ok_or_else(|| fail("IR operand is out of range"))
    };
    let check_use = |value: ValueId, block: usize, position: usize| -> Result<(), Diagnostic> {
        let Some(Some((owner, definition_position))) = definitions.get(value.0) else {
            return Err(fail("IR operand has no definition"));
        };
        if !dominators[block].contains(owner)
            || (*owner == block && *definition_position >= position)
        {
            return Err(fail("IR definition does not dominate its use"));
        }
        Ok(())
    };
    for (block_id, block) in function.blocks.iter().enumerate() {
        for (index, inst) in block.instructions.iter().enumerate() {
            for value in operands(&inst.operation) {
                check_use(*value, block_id, block.parameters.len() + index)?;
            }
            let result_ty = ty(inst.result)?;
            match &inst.operation {
                Operation::DynamicScope { globals, operands } => {
                    if globals.len() > i32::MAX as usize / 3
                        || operands.len() != globals.len() * 2 + 1
                        || result_ty != Type::Value
                        || ty(*operands
                            .last()
                            .ok_or_else(|| fail("IR dynamic scope missing body"))?)?
                            != Type::Closure(0)
                    {
                        return Err(fail("IR dynamic scope shape mismatch"));
                    }
                    for value in &operands[..globals.len()] {
                        if ty(*value)? != Type::Value {
                            return Err(fail("IR dynamic snapshot must be Value"));
                        }
                    }
                }
                Operation::Try { regions } => {
                    if result_ty != Type::Value
                        || ty(regions[0])? != Type::Closure(0)
                        || !matches!(ty(regions[1])?, Type::Nil | Type::Closure(1))
                        || !matches!(ty(regions[2])?, Type::Nil | Type::Closure(0))
                    {
                        return Err(fail("IR handler region shape mismatch"));
                    }
                }
                Operation::Literal(value) if result_ty != value.ty() => {
                    return Err(fail("IR literal result type mismatch"));
                }
                Operation::GlobalRead(_) | Operation::GlobalCell(_) if result_ty != Type::Value => {
                    return Err(fail("IR global reads require dynamic Value type"));
                }
                Operation::GlobalBound(_) if result_ty != Type::Bool => {
                    return Err(fail("IR bound checks require Bool result"));
                }
                Operation::GlobalWrite { .. } if result_ty != Type::Value => {
                    return Err(fail("IR global writes require dynamic Value result"));
                }
                Operation::MakeClosure { body, captures } => {
                    if body.arity > i32::MAX as usize
                        || body.capture_types.len() != captures.len()
                        || result_ty != Type::Closure(body.arity)
                    {
                        return Err(fail("IR closure shape mismatch"));
                    }
                    for (value, expected) in captures.iter().zip(&body.capture_types) {
                        if ty(*value)? != *expected {
                            return Err(fail("IR capture type mismatch"));
                        }
                    }
                    let expected = body
                        .capture_types
                        .len()
                        .checked_add(body.arity)
                        .ok_or_else(|| fail("IR closure entry count overflow"))?;
                    if !body
                        .function
                        .blocks
                        .first()
                        .is_some_and(|block| block.parameters.len() == expected)
                    {
                        return Err(fail("IR closure entry shape mismatch"));
                    }
                    let mut entry = body.capture_types.clone();
                    entry.extend(std::iter::repeat_n(Type::Value, body.arity));
                    verify_function(&body.function, &entry, depth + 1)?;
                }
                Operation::MakeGeneralClosure { body, captures } => {
                    if body.methods.is_empty()
                        || body.capture_types.len() != captures.len()
                        || result_ty != Type::Value
                    {
                        return Err(fail("IR general closure shape mismatch"));
                    }
                    for (value, expected) in captures.iter().zip(&body.capture_types) {
                        if ty(*value)? != *expected {
                            return Err(fail("IR capture type mismatch"));
                        }
                    }
                    let mut environment = body.capture_types.clone();
                    if body.self_capture {
                        environment.push(Type::Value);
                    }
                    let mut arities = HashSet::new();
                    for method in &body.methods {
                        if method.arity > i32::MAX as usize
                            || method.capture_types != environment
                            || !arities.insert(method.arity)
                        {
                            return Err(fail("IR general closure method shape mismatch"));
                        }
                        let expected = environment
                            .len()
                            .checked_add(method.arity)
                            .ok_or_else(|| fail("IR closure entry count overflow"))?;
                        if !method
                            .function
                            .blocks
                            .first()
                            .is_some_and(|block| block.parameters.len() == expected)
                        {
                            return Err(fail("IR closure entry shape mismatch"));
                        }
                        let mut entry = environment.clone();
                        entry.extend(std::iter::repeat_n(Type::Value, method.arity));
                        verify_function(&method.function, &entry, depth + 1)?;
                    }
                }
                Operation::Call { operands } => {
                    if operands.is_empty() || result_ty != Type::Value {
                        return Err(fail("IR call requires callee and dynamic result"));
                    }
                    if let Type::Closure(arity) = ty(operands[0])? {
                        if arity != operands.len() - 1 {
                            return Err(fail("IR known closure call arity mismatch"));
                        }
                    }
                }
                Operation::NilTest(_) => {
                    if result_ty != Type::Bool {
                        return Err(fail("IR nil-test result must be Boolean"));
                    }
                }
                Operation::Bitwise {
                    operation,
                    arguments,
                } => {
                    if arguments.len() != operation.arity() || result_ty != operation.result() {
                        return Err(fail("Invalid bitwise IR shape"));
                    }
                }
                Operation::Comparison { arguments, .. } => {
                    if arguments.len() != 2 || result_ty != Type::Bool {
                        return Err(fail("Invalid comparison IR shape"));
                    }
                }
                Operation::Array {
                    operation,
                    arguments,
                } => {
                    if !operation.valid(arguments.len()) || result_ty != Type::Value {
                        return Err(fail("Invalid array IR shape"));
                    }
                }
                Operation::Nominal {
                    operation,
                    arguments,
                } => {
                    let types = arguments
                        .iter()
                        .map(|argument| ty(*argument))
                        .collect::<Result<Vec<_>, _>>()?;
                    if !operation.valid(&types) || result_ty != operation.result() {
                        return Err(fail("Invalid nominal IR shape"));
                    }
                }
                Operation::Arithmetic {
                    operator,
                    arguments,
                } => {
                    let arity = if *operator == Arithmetic::Negate {
                        1
                    } else {
                        2
                    };
                    if arguments.len() != arity {
                        return Err(fail("IR intrinsic arity mismatch"));
                    }
                    let operand_types = arguments
                        .iter()
                        .map(|arg| ty(*arg))
                        .collect::<Result<Vec<_>, _>>()?;
                    if arithmetic_type(*operator, &operand_types) != Some(result_ty) {
                        return Err(fail("IR arithmetic operand/result type mismatch"));
                    }
                }
                _ => {}
            }
        }
        let position = block.parameters.len() + block.instructions.len();
        match &block.terminator {
            Terminator::Return(value) | Terminator::Throw(value) => {
                check_use(*value, block_id, position)?
            }
            Terminator::Jump { target, arguments } => {
                let parameters = &function.blocks[*target].parameters;
                if parameters.len() != arguments.len() {
                    return Err(fail("IR edge arity mismatch"));
                }
                for (value, param) in arguments.iter().zip(parameters) {
                    check_use(*value, block_id, position)?;
                    if !ty(*param)?.accepts(ty(*value)?) {
                        return Err(fail("IR edge type mismatch"));
                    }
                }
            }
            Terminator::Branch {
                condition,
                consequent,
                alternative,
            } => {
                check_use(*condition, block_id, position)?;
                if !function.blocks[*consequent].parameters.is_empty()
                    || !function.blocks[*alternative].parameters.is_empty()
                {
                    return Err(fail("IR branch cannot omit edge arguments"));
                }
            }
        }
    }
    Ok(())
}
