//! Explicit values, blocks and edge parameters. Verification precedes emission.
pub mod liveness;
pub mod r#async;
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
    /// An evaluated future operand. Its result exists only on successful resume;
    /// continuation normalization removes this instruction at a state boundary.
    Await { future: ValueId },
    MakeFuture { body: Box<FutureBody>, captures: Vec<ValueId> },
    /// Compiled region entry. Exceptional successors are represented by latent
    /// CFG branches; emission always enters the body on the normal path.
    RegionPush { handler: Option<usize>, cleanup: Option<usize>, end: usize, dynamic: Vec<ValueId> },
    RegionExit { value: ValueId, cleanup: bool },
    RegionValue,
    AsyncDynamicEnter { globals: Vec<Global>, operands: Vec<ValueId> },
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
pub struct Function {
    pub values: Vec<Value>,
    pub blocks: Vec<Block>,
    pub span: Range<usize>,
}
impl Clone for Function {
    fn clone(&self) -> Self {
        stacker::maybe_grow(64 * 1024, 1024 * 1024, || Self {
            values: self.values.clone(), blocks: self.blocks.clone(), span: self.span.clone(),
        })
    }
}
impl Drop for Function {
    fn drop(&mut self) {
        fn detach(blocks: Vec<Block>, pending: &mut Vec<Function>) {
            for block in blocks {
                for instruction in block.instructions {
                    match instruction.operation {
                        Operation::MakeClosure { body, .. } => pending.push(body.function),
                        Operation::MakeFuture { body, .. } => pending.push(body.continuation.function),
                        Operation::MakeGeneralClosure { body, .. } => {
                            pending.extend(body.methods.into_iter().map(|method| method.function));
                        }
                        _ => {}
                    }
                }
            }
        }
        if self.blocks.is_empty() { return; }
        let mut pending = Vec::new();
        detach(std::mem::take(&mut self.blocks), &mut pending);
        while let Some(mut function) = pending.pop() {
            detach(std::mem::take(&mut function.blocks), &mut pending);
        }
    }
}
impl std::fmt::Debug for Function {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        stacker::maybe_grow(64 * 1024, 1024 * 1024, || {
            f.debug_struct("Function").field("values", &self.values)
                .field("blocks", &self.blocks).field("span", &self.span).finish()
        })
    }
}
#[derive(Debug, Clone)]
pub struct FutureBody {
    pub capture_types: Vec<Type>,
    pub continuation: r#async::Continuation,
}
#[derive(Debug, Clone)]
pub struct ClosureBody {
    pub display_name: Option<Vec<u16>>,
    pub variadic: bool,
    pub capture_types: Vec<Type>,
    pub arity: usize,
    pub function: Function,
}
#[derive(Debug, Clone)]
pub struct GeneralClosureBody {
    pub display_name: Option<Vec<u16>>,
    pub capture_types: Vec<Type>,
    pub methods: Vec<ClosureBody>,
    pub self_capture: bool,
    pub rest_class: Option<Global>,
}
struct Lowerer<'a> {
    suspendable: bool,
    function: Function,
    current: usize,
    bindings: HashMap<BindingId, ValueId>,
    targets: HashMap<LoopId, (usize, usize)>,
    meter: &'a mut dyn FnMut(u64) -> Result<(), Diagnostic>,
}
impl<'a> Lowerer<'a> {
    fn new(
        span: Range<usize>,
        meter: &'a mut dyn FnMut(u64) -> Result<(), Diagnostic>,
    ) -> Result<Self, Diagnostic> {
        lower_charge(meter, 6, &span)?;
        let mut lowerer = Self {
            suspendable: false,
            function: Function {
                values: Vec::new(),
                blocks: Vec::new(),
                span,
            },
            current: 0,
            bindings: HashMap::new(),
            targets: HashMap::new(),
            meter,
        };
        lowerer.function.blocks.push(Block {
            parameters: Vec::new(),
            instructions: Vec::new(),
            terminator: Terminator::Return(ValueId(usize::MAX)),
        });
        Ok(lowerer)
    }
    fn parameter(&mut self, id: BindingId, ty: Type, span: Range<usize>) -> Result<(), Diagnostic> {
        self.charge(2, &span)?;
        let value = self.value(ty, span.clone())?;
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
            self.charge(1, &hir.span)?;
            self.function.blocks[self.current].terminator = Terminator::Return(value);
        }
        Ok(self.function)
    }
    fn value(&mut self, ty: Type, span: Range<usize>) -> Result<ValueId, Diagnostic> {
        self.charge(1, &span)?;
        let id = ValueId(self.function.values.len());
        self.function.values.push(Value { ty, span });
        Ok(id)
    }
    fn block(&mut self, parameters: Vec<ValueId>) -> Result<usize, Diagnostic> {
        self.charge(1usize.saturating_add(parameters.len()), &self.function.span.clone())?;
        let id = self.function.blocks.len();
        self.function.blocks.push(Block {
            parameters,
            instructions: Vec::new(),
            terminator: Terminator::Return(ValueId(usize::MAX)),
        });
        Ok(id)
    }
    fn charge(&mut self, work: usize, span: &Range<usize>) -> Result<(), Diagnostic> {
        lower_charge(self.meter, work, span)
    }
    fn clone_global(&mut self, global: &Global, span: &Range<usize>) -> Result<Global, Diagnostic> {
        global.retain_metered(&mut |work| (self.meter)(work).map_err(|mut error| {
            error.span = span.clone(); error
        }))
    }
    fn display_name(&mut self, hir: &Hir) -> Result<Option<Vec<u16>>, Diagnostic> {
        if let Some(scope) = hir
            .source
            .as_ref()
            .and_then(|source| source.callable.as_ref())
            .and_then(|callable| callable.name.as_ref())
        {
            // Munging can expand one source character into a longer spelling
            // (for example, `-` becomes `_`). Charge an upper bound before the
            // helper allocates its intermediate strings and UTF-16 result.
            let expansion = |bytes: usize| bytes.saturating_mul(16);
            self.charge(expansion(scope.namespace.len()), &hir.span)?;
            for parent in scope.parents.iter().chain(std::iter::once(scope)) {
                self.charge(1, &hir.span)?;
                if let suss_reader::forms::Kind::Symbol(symbol) = &parent.declaration.kind {
                    self.charge(expansion(symbol.name.len()), &hir.span)?;
                    if let Some(namespace) = &symbol.namespace {
                        self.charge(expansion(namespace.len()), &hir.span)?;
                    }
                }
            }
        }
        Ok(super::function_names::display_name(hir))
    }
    fn emit(&mut self, operation: Operation, ty: Type, span: Range<usize>) -> Result<ValueId, Diagnostic> {
        self.charge(1usize.saturating_add(operands(&operation).len()), &span)?;
        let result = self.value(ty, span.clone())?;
        self.function.blocks[self.current]
            .instructions
            .push(Instruction {
                result,
                operation,
                span,
            });
        Ok(result)
    }
    fn literal(&mut self, value: Literal, span: Range<usize>) -> Result<ValueId, Diagnostic> {
        let ty = value.ty();
        self.emit(Operation::Literal(value), ty, span)
    }
    fn expression(&mut self, hir: &Hir) -> Result<Option<ValueId>, Diagnostic> {
        self.charge(expression_preallocation_work(hir), &hir.span)?;
        stacker::maybe_grow(256 * 1024, 4 * 1024 * 1024, || self.expression_inner(hir))
    }
    fn expression_inner(&mut self, hir: &Hir) -> Result<Option<ValueId>, Diagnostic> {
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
            Expression::Await { value } if self.suspendable => {
                let future = operand!(value);
                self.emit(Operation::Await { future }, Type::Value, hir.span.clone())?
            }
            Expression::Future { captures, body } => {
                let mut captured = Vec::new();
                let mut typed = Vec::new();
                for id in captures {
                    let value = *self.bindings.get(id).ok_or_else(|| Diagnostic {
                        span: hir.span.clone(), message: "Undefined HIR future capture".into(),
                    })?;
                    captured.push(value);
                    typed.push((*id, self.function.values[value.0].ty));
                }
                let continuation =
                    r#async::lower_metered(body, &typed, &mut *self.meter)?;
                self.emit(Operation::MakeFuture {
                    body: Box::new(FutureBody {
                        capture_types: typed.iter().map(|(_, ty)| *ty).collect(), continuation,
                    }), captures: captured,
                }, Type::Value, hir.span.clone())?
            }
            Expression::AsyncTry { body, handler, cleanup, payload } if self.suspendable => {
                return self.async_region(hir, body, handler.as_deref(), cleanup.as_deref(), payload.as_ref(), Vec::new());
            }
            Expression::AsyncDynamicScope { bindings, body } if self.suspendable => {
                let mut globals = Vec::new();
                let mut operands = Vec::new();
                for (global, _) in bindings {
                    globals.push(self.clone_global(global, &hir.span)?);
                    let global = self.clone_global(global, &hir.span)?;
                    operands.push(self.emit(Operation::GlobalRead(global), Type::Value, hir.span.clone())?);
                }
                for (_, value) in bindings { operands.push(operand!(value)); }
                let frame = self.emit(Operation::AsyncDynamicEnter { globals, operands }, Type::Value, hir.span.clone())?;
                return self.async_region(hir, body, None, None, None, vec![frame]);
            }
            Expression::Await { .. }
            | Expression::AsyncTry { .. } | Expression::AsyncDynamicScope { .. } => {
                return Err(Diagnostic { span: hir.span.clone(),
                    message: "Async continuation lowering is not implemented".into() });
            }
            Expression::Assign { global, value } => {
                let value = operand!(value);
                let global = self.clone_global(global, &hir.span)?;
                self.emit(
                    Operation::GlobalWrite {
                        global,
                        value,
                    },
                    Type::Value,
                    hir.span.clone(),
                )?
            }
            Expression::DynamicScope { bindings, body } => {
                let globals = bindings
                    .iter()
                    .map(|(global, _)| self.clone_global(global, &hir.span))
                    .collect::<Result<Vec<_>, Diagnostic>>()?;
                let mut operands = globals
                    .iter()
                    .map(|global| {
                        let global = self.clone_global(global, &hir.span)?;
                        self.emit(
                            Operation::GlobalRead(global),
                            Type::Value,
                            hir.span.clone(),
                        )
                    })
                    .collect::<Result<Vec<_>, Diagnostic>>()?;
                for (_, value) in bindings {
                    operands.push(operand!(value));
                }
                operands.push(operand!(body));
                self.emit(
                    Operation::DynamicScope { globals, operands },
                    Type::Value,
                    hir.span.clone(),
                )?
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
                self.emit(Operation::Try { regions }, Type::Value, hir.span.clone())?
            }
            Expression::Literal(value) => self.literal(value.clone(), hir.span.clone())?,
            Expression::GlobalCell(global) => {
                if hir.ty != Type::Value {
                    return Err(Diagnostic {
                        span: hir.span.clone(),
                        message: "Invalid cell-reference HIR type".into(),
                    });
                }
                let global = self.clone_global(global, &hir.span)?;
                self.emit(
                    Operation::GlobalCell(global),
                    Type::Value,
                    hir.span.clone(),
                )?
            }
            Expression::Global(global) => {
                let global = self.clone_global(global, &hir.span)?;
                self.emit(
                    Operation::GlobalRead(global),
                    Type::Value,
                    hir.span.clone(),
                )?
            }
            Expression::GlobalOrFallback { global, fallback } => {
                let global_cell = self.clone_global(global, &hir.span)?;
                let condition = self.emit(Operation::GlobalBound(global_cell), Type::Bool, hir.span.clone())?;
                let result = self.value(Type::Value, hir.span.clone())?;
                let bound = self.block(Vec::new())?;
                let unbound = self.block(Vec::new())?;
                let join = self.block(vec![result])?;
                self.function.blocks[self.current].terminator = Terminator::Branch {
                    condition, consequent: bound, alternative: unbound,
                };
                for (block, cell) in [(bound, global), (unbound, fallback)] {
                    self.current = block;
                    let cell = self.clone_global(cell, &hir.span)?;
                    let value = self.emit(Operation::GlobalRead(cell), Type::Value, hir.span.clone())?;
                    self.function.blocks[self.current].terminator = Terminator::Jump { target: join, arguments: vec![value] };
                }
                self.current = join;
                result
            }
            Expression::Definition {
                global,
                initializer,
                once,
                ..
            } => {
                if let Some(initializer) = initializer {
                    if *once {
                        let global_for_test = self.clone_global(global, &hir.span)?;
                        let condition = self.emit(
                            Operation::GlobalBound(global_for_test),
                            Type::Bool,
                            hir.span.clone(),
                        )?;
                        let result = self.value(Type::Value, hir.span.clone())?;
                        let bound = self.block(Vec::new())?;
                        let unbound = self.block(Vec::new())?;
                        let join = self.block(vec![result])?;
                        self.function.blocks[self.current].terminator = Terminator::Branch {
                            condition,
                            consequent: bound,
                            alternative: unbound,
                        };
                        self.current = bound;
                        let nil = self.literal(Literal::Nil, hir.span.clone())?;
                        self.function.blocks[self.current].terminator = Terminator::Jump {
                            target: join,
                            arguments: vec![nil],
                        };
                        self.current = unbound;
                        if let Some(value) = self.expression(initializer)? {
                            let global_for_write = self.clone_global(global, &hir.span)?;
                            let value = self.emit(
                                Operation::GlobalWrite {
                                    global: global_for_write,
                                    value,
                                },
                                Type::Value,
                                hir.span.clone(),
                            )?;
                            self.function.blocks[self.current].terminator = Terminator::Jump {
                                target: join,
                                arguments: vec![value],
                            };
                        }
                        self.current = join;
                        result
                    } else {
                        let value = operand!(initializer);
                        let global = self.clone_global(global, &hir.span)?;
                        self.emit(
                            Operation::GlobalWrite {
                                global,
                                value,
                            },
                            Type::Value,
                            hir.span.clone(),
                        )?
                    }
                } else {
                    // A declaration does not reset an existing cell or bind an unbound one.
                    if *once {
                        return Err(Diagnostic {
                            span: hir.span.clone(),
                            message: "HIR defonce requires initializer".into(),
                        });
                    }
                    self.literal(Literal::Nil, hir.span.clone())?
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
                self.emit(Operation::Call { operands }, Type::Value, hir.span.clone())?
            }
            Expression::Function {
                parameters,
                captures,
                body,
            } => {
                self.charge(captures.len().saturating_mul(2), &hir.span)?;
                let mut captured = Vec::new();
                let mut capture_types = Vec::new();
                let mut inner_parameters = Vec::with_capacity(captures.len() + parameters.len());
                for id in captures {
                    let value = *self.bindings.get(id).ok_or_else(|| Diagnostic {
                        span: hir.span.clone(),
                        message: "Undefined HIR capture".into(),
                    })?;
                    let ty = self.function.values[value.0].ty;
                    captured.push(value);
                    capture_types.push(ty);
                    inner_parameters.push((*id, ty, hir.span.clone()));
                }
                for parameter in parameters {
                    inner_parameters.push((parameter.id, Type::Value, parameter.span.clone()));
                }
                let mut inner = Lowerer::new(body.span.clone(), &mut *self.meter)?;
                for (id, ty, span) in inner_parameters {
                    inner.parameter(id, ty, span)?;
                }
                let function = inner.finish(body)?;
                let display_name = self.display_name(hir)?;
                self.emit(
                    Operation::MakeClosure {
                        body: Box::new(ClosureBody {
                            display_name,
                            variadic: false,
                            capture_types,
                            arity: parameters.len(),
                            function,
                        }),
                        captures: captured,
                    },
                    Type::Closure(parameters.len()),
                    hir.span.clone(),
                )?
            }
            Expression::GeneralFunction {
                methods,
                captures,
                self_binding,
                rest_class,
            } => {
                self.charge(captures.len().saturating_mul(2), &hir.span)?;
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
                self.charge(methods.len(), &hir.span)?;
                let mut lowered = Vec::new();
                for method in methods {
                    self.charge(
                        captures
                            .len()
                            .saturating_add(method.parameters.len())
                            .saturating_add(usize::from(self_binding.is_some())),
                        &method.body.span,
                    )?;
                    let mut inner =
                        Lowerer::new(method.body.span.clone(), &mut *self.meter)?;
                    for (id, ty) in captures.iter().zip(&capture_types) {
                        inner.parameter(*id, *ty, hir.span.clone())?;
                    }
                    inner.charge(capture_types.len(), &method.body.span)?;
                    let mut environment_types = capture_types.clone();
                    if let Some(parameter) = self_binding {
                        inner.parameter(parameter.id, Type::Value, parameter.span.clone())?;
                        environment_types.push(Type::Value);
                    }
                    for parameter in &method.parameters {
                        inner.parameter(parameter.id, Type::Value, parameter.span.clone())?;
                    }
                    lowered.push(ClosureBody {
                        display_name: None,
                        variadic: method.variadic,
                        capture_types: environment_types,
                        arity: method.parameters.len(),
                        function: inner.finish(&method.body)?,
                    });
                }
                let display_name = self.display_name(hir)?;
                let rest_class = rest_class
                    .as_ref()
                    .map(|global| self.clone_global(global, &hir.span))
                    .transpose()?;
                self.emit(
                    Operation::MakeGeneralClosure {
                        body: Box::new(GeneralClosureBody {
                            display_name,
                            capture_types,
                            methods: lowered,
                            self_capture: self_binding.is_some(),
                            rest_class,
                        }),
                        captures: captured,
                    },
                    Type::Value,
                    hir.span.clone(),
                )?
            }
            Expression::Do(items) => {
                if items.is_empty() {
                    self.literal(Literal::Nil, hir.span.clone())?
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
                    self.charge(1, &binding.span)?;
                    if self.bindings.insert(binding.id, value).is_some() {
                        return Err(Diagnostic {
                            span: binding.span.clone(),
                            message: "HIR binding identity is defined twice".into(),
                        });
                    }
                    initial.push(value);
                }
                for binding in bindings {
                    parameters.push(self.value(Type::Value, binding.span.clone())?);
                }
                let header = self.block(parameters.clone())?;
                self.function.blocks[self.current].terminator = Terminator::Jump {
                    target: header,
                    arguments: initial,
                };
                for (binding, value) in bindings.iter().zip(parameters) {
                    self.charge(1, &binding.span)?;
                    self.bindings.insert(binding.id, value);
                }
                self.charge(1, &hir.span)?;
                self.targets.insert(*target, (header, bindings.len()));
                self.current = header;
                let result = self.expression(body);
                self.charge(1, &hir.span)?;
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
                for (index, binding) in bindings.iter().enumerate() {
                    let value = operand!(&binding.value);
                    // General source functions are wrapped to attach their
                    // callable signatures. Their actual source callable facts
                    // belong to the outer node; the first binding constructs the
                    // owner. Transfer only its display label, without attributing
                    // those facts to synthetic delegates or adding lexical IDs.
                    if index == 0
                        && binding.value.source.is_none()
                        && matches!(binding.value.kind, Expression::GeneralFunction { .. })
                        && let Some(name) = self.display_name(hir)?
                    {
                        let instruction = self.function.blocks[self.current].instructions.last_mut()
                            .expect("general function owner instruction");
                        debug_assert_eq!(instruction.result, value);
                        let Operation::MakeGeneralClosure { body, .. } = &mut instruction.operation else {
                            unreachable!("general function owner construction");
                        };
                        body.display_name = Some(name);
                    }
                    self.charge(1, &binding.span)?;
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
                let then_block = self.block(Vec::new())?;
                let else_block = self.block(Vec::new())?;
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
                let result = self.value(hir.ty, hir.span.clone())?;
                let join = self.block(vec![result])?;
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
                self.emit(Operation::NilTest(value), Type::Bool, hir.span.clone())?
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
                )?
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
                )?
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
                )?
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
                )?
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
                    )?));
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
                        )?));
                    }
                    if *operator == Arithmetic::Divide {
                        let one = self.literal(Literal::Number(1.0), hir.span.clone())?;
                        return Ok(Some(self.emit(
                            Operation::Arithmetic {
                                operator: *operator,
                                arguments: vec![one, result],
                            },
                            Type::Number,
                            hir.span.clone(),
                        )?));
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
                    )?;
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
    meter: &mut dyn FnMut(u64) -> Result<(), Diagnostic>,
) -> Result<(), Diagnostic> {
    lower_charge(meter, 1, &hir.span)?;
    stacker::maybe_grow(256 * 1024, 4 * 1024 * 1024, || {
        verify_recurrence_inner(hir, targets, tail, meter)
    })
}
fn verify_recurrence_inner(
    hir: &Hir,
    targets: &mut Vec<(LoopId, usize)>,
    tail: bool,
    meter: &mut dyn FnMut(u64) -> Result<(), Diagnostic>,
) -> Result<(), Diagnostic> {
    let error = |message: &str| Diagnostic {
        span: hir.span.clone(),
        message: message.into(),
    };
    match &hir.kind {
        Expression::Future { body, .. } => {
            verify_recurrence(body, &mut Vec::new(), false, meter)?;
        }
        Expression::Await { value } => {
            verify_recurrence(value, targets, false, meter)?;
        }
        Expression::AsyncTry { body, handler, cleanup, .. } => {
            verify_recurrence(body, &mut Vec::new(), false, meter)?;
            if let Some(handler) = handler { verify_recurrence(handler, &mut Vec::new(), false, meter)?; }
            if let Some(cleanup) = cleanup { verify_recurrence(cleanup, &mut Vec::new(), false, meter)?; }
        }
        Expression::AsyncDynamicScope { bindings, body } => {
            for (_, value) in bindings { verify_recurrence(value, targets, false, meter)?; }
            verify_recurrence(body, &mut Vec::new(), false, meter)?;
        }
        Expression::Assign { value, .. } => verify_recurrence(value, targets, false, meter)?,
        Expression::DynamicScope { bindings, body } => {
            if hir.ty != Type::Value
                || !matches!(&body.kind, Expression::Function { parameters, .. } if parameters.is_empty())
            {
                return Err(error("HIR dynamic scope shape mismatch"));
            }
            for (_, value) in bindings {
                verify_recurrence(value, targets, false, meter)?;
            }
            verify_recurrence(body, &mut Vec::new(), false, meter)?;
        }
        Expression::Throw(value) => verify_recurrence(value, targets, false, meter)?,
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
                verify_recurrence(region, &mut Vec::new(), false, meter)?;
            }
        }
        Expression::Loop {
            target,
            bindings,
            body,
        } => {
            for (id, _) in targets.iter() {
                lower_charge(meter, 1, &hir.span)?;
                if id == target {
                    return Err(error("HIR recurrence target defined twice"));
                }
            }
            for binding in bindings {
                verify_recurrence(&binding.value, targets, false, meter)?;
            }
            lower_charge(meter, 1, &hir.span)?;
            targets.push((*target, bindings.len()));
            let result = verify_recurrence(body, targets, true, meter);
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
                verify_recurrence(argument, targets, false, meter)?;
            }
        }
        Expression::Function { body, .. } => verify_recurrence(body, &mut Vec::new(), true, meter)?,
        Expression::GeneralFunction { methods, .. } => {
            if methods.is_empty() {
                return Err(error("HIR function requires a signature"));
            }
            for method in methods {
                verify_recurrence(&method.body, &mut Vec::new(), true, meter)?;
            }
        }
        Expression::Let { bindings, body } => {
            for binding in bindings {
                verify_recurrence(&binding.value, targets, false, meter)?;
            }
            verify_recurrence(body, targets, tail, meter)?;
        }
        Expression::Do(items) => {
            for (index, item) in items.iter().enumerate() {
                verify_recurrence(item, targets, tail && index + 1 == items.len(), meter)?;
            }
        }
        Expression::If {
            condition,
            consequent,
            alternative,
        } => {
            verify_recurrence(condition, targets, false, meter)?;
            verify_recurrence(consequent, targets, tail, meter)?;
            verify_recurrence(alternative, targets, tail, meter)?;
        }
        Expression::Call { callee, arguments } => {
            verify_recurrence(callee, targets, false, meter)?;
            for argument in arguments {
                verify_recurrence(argument, targets, false, meter)?;
            }
        }
        Expression::NilTest(value) => verify_recurrence(value, targets, false, meter)?,
        Expression::Arithmetic { arguments, .. }
        | Expression::Bitwise { arguments, .. }
        | Expression::Comparison { arguments, .. }
        | Expression::Array { arguments, .. }
        | Expression::Nominal { arguments, .. } => {
            for argument in arguments {
                verify_recurrence(argument, targets, false, meter)?;
            }
        }
        Expression::Definition {
            initializer: Some(value),
            ..
        } => verify_recurrence(value, targets, false, meter)?,
        _ => {}
    }
    Ok(())
}

pub fn lower(hir: &Hir) -> Result<Function, Diagnostic> {
    let mut remaining = super::DEFAULT_OPERATION_FUEL;
    let span = hir.span.clone();
    let mut meter = |work: u64| {
        remaining = remaining.checked_sub(work).ok_or_else(|| Diagnostic {
            span: span.clone(),
            message: "IR lowering fuel exhausted".into(),
        })?;
        Ok(())
    };
    lower_metered(hir, &mut meter)
}

/// Lower HIR while charging recurrence validation, IR construction, and nested
/// continuation normalization to the caller's cumulative operation budget.
pub fn lower_metered(
    hir: &Hir,
    meter: &mut dyn FnMut(u64) -> Result<(), Diagnostic>,
) -> Result<Function, Diagnostic> {
    verify_recurrence(hir, &mut Vec::new(), false, meter)?;
    Lowerer::new(hir.span.clone(), meter)?.finish(hir)
}

fn lower_charge(
    meter: &mut dyn FnMut(u64) -> Result<(), Diagnostic>,
    amount: usize,
    span: &Range<usize>,
) -> Result<(), Diagnostic> {
    if amount == 0 {
        return Ok(());
    }
    meter(u64::try_from(amount).unwrap_or(u64::MAX)).map_err(|mut error| {
        error.span = span.clone();
        error
    })
}

fn expression_preallocation_work(hir: &Hir) -> usize {
    use Expression::*;
    // One unit visits this node. Explicit Value/Block/Instruction allocations
    // are charged by Lowerer helpers; this covers HIR-sized temporary vectors,
    // parallel copies, and CFG terminator argument buffers before they exist.
    let base = 1usize;
    let sized = match &hir.kind {
        Future { captures, .. } => captures.len().saturating_mul(3),
        Literal(super::hir::Literal::String(text)) => text.len(),
        AsyncTry { handler, cleanup, .. } => 8usize
            .saturating_add(usize::from(handler.is_some()))
            .saturating_add(usize::from(cleanup.is_some())),
        AsyncDynamicScope { bindings, .. } => bindings.len().saturating_mul(3).saturating_add(5),
        DynamicScope { bindings, .. } => bindings.len().saturating_mul(3).saturating_add(1),
        Try { .. } => 0,
        GlobalOrFallback { .. } | Definition { once: true, initializer: Some(_), .. } => 2,
        Call { arguments, .. } => arguments.len().saturating_add(1),
        Function { parameters, captures, .. } => parameters.len()
            .saturating_add(captures.len().saturating_mul(3))
            .saturating_add(2),
        // Charge method count here in O(1); parameter/capture copies are charged
        // incrementally immediately before each method's allocations below.
        GeneralFunction { methods, .. } => methods.len(),
        Loop { bindings, .. } => bindings.len().saturating_mul(3).saturating_add(1),
        Recur { arguments, .. } => arguments.len(),
        Do(_) | Let { .. } => 0,
        If { .. } => 3,
        Bitwise { arguments, .. }
        | Comparison { arguments, .. }
        | Array { arguments, .. }
        | Nominal { arguments, .. } => arguments.len().saturating_mul(2),
        Arithmetic { arguments, .. } => arguments.len().saturating_mul(3),
        _ => 1,
    };
    base.saturating_add(sized)
}

pub(super) fn operands(operation: &Operation) -> &[ValueId] {
    match operation {
        Operation::Literal(_)
        | Operation::GlobalRead(_)
        | Operation::GlobalCell(_)
        | Operation::GlobalBound(_) | Operation::RegionValue => &[],
        Operation::GlobalWrite { value, .. } | Operation::NilTest(value)
        | Operation::Await { future: value } | Operation::RegionExit { value, .. } => {
            std::slice::from_ref(value)
        }
        Operation::Arithmetic { arguments, .. }
        | Operation::Bitwise { arguments, .. }
        | Operation::Comparison { arguments, .. }
        | Operation::Array { arguments, .. }
        | Operation::Nominal { arguments, .. } => arguments,
        Operation::MakeFuture { captures, .. }
        | Operation::MakeClosure { captures, .. }
        | Operation::MakeGeneralClosure { captures, .. } => captures,
        Operation::RegionPush { dynamic, .. } => dynamic,
        Operation::Call { operands } | Operation::DynamicScope { operands, .. }
        | Operation::AsyncDynamicEnter { operands, .. } => operands,
        Operation::Try { regions } => regions,
    }
}

#[cfg(test)]
mod metering_tests {
    use super::*;

    fn nil_hir() -> Hir {
        Hir {
            source: None,
            span: 3..4,
            metadata: Vec::new(),
            ty: Type::Nil,
            kind: Expression::Literal(Literal::Nil),
        }
    }

    #[test]
    fn nested_loop_target_scans_cannot_bypass_lowering_fuel() {
        let mut hir = nil_hir();
        for id in 0..64 {
            hir = Hir {
                source: None, span: 3..4, metadata: Vec::new(), ty: Type::Nil,
                kind: Expression::Loop {
                    target: LoopId(id), bindings: Vec::new(), body: Box::new(hir),
                },
            };
        }
        let mut remaining = 1000u64;
        let error = lower_metered(&hir, &mut |work| {
            remaining = remaining.checked_sub(work).ok_or_else(|| Diagnostic {
                span: 0..0, message: "test target scan fuel exhausted".into(),
            })?;
            Ok(())
        }).unwrap_err();
        assert_eq!(error.message, "test target scan fuel exhausted");
        assert_eq!(error.span, 3..4);
        let function = lower(&hir).unwrap();
        verify(&function).unwrap();
    }

    #[test]
    fn lower_metered_charges_validation_and_ir_and_allows_retry() {
        let hir = nil_hir();
        let mut remaining = 8u64;
        let mut charges = 0usize;
        let error = lower_metered(&hir, &mut |work| {
            charges += 1;
            remaining = remaining.checked_sub(work).ok_or_else(|| Diagnostic {
                span: hir.span.clone(),
                message: "test lowering cancellation".into(),
            })?;
            Ok(())
        })
        .unwrap_err();
        assert_eq!(error.message, "test lowering cancellation");
        assert_eq!(error.span, hir.span);
        assert!(charges >= 3, "recurrence and IR construction must share the meter");

        // The failed request owns no persistent lowering state; a new request
        // with a fresh meter can lower the same analyzed HIR.
        let mut work = 0u64;
        let function = lower_metered(&hir, &mut |amount| {
            work += amount;
            Ok(())
        })
        .unwrap();
        assert!(work > 8);
        assert_eq!(function.blocks.len(), 1);
        assert_eq!(function.values.len(), 1);
    }
}
/// Check graph integrity, unique definitions, dominance, edge arities and types.
/// Calls have ordered operands, verified closure/capture shape and known arities.
/// General exception/effect/suspension and tail-position analysis remain incomplete.
pub fn verify(function: &Function) -> Result<(), Diagnostic> {
    verify_with_entry(function, &[])
}
fn verify_with_entry(function: &Function, entry: &[Type]) -> Result<(), Diagnostic> {
    let mut remaining = super::DEFAULT_OPERATION_FUEL;
    verify_function(function, entry, &mut |work| {
        remaining = remaining.checked_sub(work).ok_or_else(|| Diagnostic {
            span: function.span.clone(),
            message: "IR verification fuel exhausted".into(),
        })?;
        Ok(())
    })
}
/// Verify using the caller's cumulative computation budget. No source nesting
/// limit is imposed; recursive closure checks grow the native stack as needed.
pub fn verify_metered(
    function: &Function,
    meter: &mut dyn FnMut(u64) -> Result<(), Diagnostic>,
) -> Result<(), Diagnostic> {
    verify_function(function, &[], meter)
}
fn verification_work(
    meter: &mut dyn FnMut(u64) -> Result<(), Diagnostic>,
    amount: usize,
    span: &Range<usize>,
) -> Result<(), Diagnostic> {
    meter(u64::try_from(amount).unwrap_or(u64::MAX)).map_err(|mut error| {
        error.span = span.clone();
        error
    })
}
fn verify_function(
    function: &Function,
    entry_types: &[Type],
    meter: &mut dyn FnMut(u64) -> Result<(), Diagnostic>,
) -> Result<(), Diagnostic> {
    stacker::maybe_grow(64 * 1024, 1024 * 1024, || {
        verify_function_inner(function, entry_types, meter)
    })
}
fn verify_function_inner(
    function: &Function,
    entry_types: &[Type],
    meter: &mut dyn FnMut(u64) -> Result<(), Diagnostic>,
) -> Result<(), Diagnostic> {
    let fail = |message: &str| Diagnostic {
        span: function.span.clone(),
        message: message.into(),
    };
    let n = function.blocks.len();
    verification_work(meter, 1, &function.span)?;
    if n == 0 || function.blocks[0].parameters.len() != entry_types.len() {
        return Err(fail("IR entry parameter shape mismatch"));
    }
    verification_work(meter, entry_types.len(), &function.span)?;
    for (id, expected) in function.blocks[0].parameters.iter().zip(entry_types) {
        if !function
            .values
            .get(id.0)
            .is_some_and(|value| value.ty == *expected)
        {
            return Err(fail("IR entry parameter type mismatch"));
        }
    }
    verification_work(meter, function.values.len(), &function.span)?;
    verification_work(meter, n.saturating_mul(2), &function.span)?;
    let mut definitions = vec![None; function.values.len()];
    let mut predecessors = vec![Vec::new(); n];
    let mut successors = vec![Vec::new(); n];
    for (block_id, block) in function.blocks.iter().enumerate() {
        verification_work(meter, 1, &function.span)?;
        for (position, value) in block
            .parameters
            .iter()
            .chain(block.instructions.iter().map(|inst| &inst.result))
            .enumerate()
        {
            verification_work(meter, 1, &function.span)?;
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
            verification_work(meter, 1, &function.span)?;
            if target >= n {
                return Err(fail("IR edge is out of range"));
            }
            predecessors[target].push(block_id);
            successors[block_id].push(target);
        }
    }
    verification_work(meter, definitions.len(), &function.span)?;
    if definitions.iter().any(Option::is_none) {
        return Err(fail("IR value has no definition"));
    }
    let mut reachable = HashSet::new();
    let mut work = vec![0];
    while let Some(id) = work.pop() {
        verification_work(meter, 1, &function.span)?;
        if reachable.insert(id) {
            verification_work(meter, successors[id].len(), &function.span)?;
            work.extend(&successors[id]);
        }
    }
    if reachable.len() != n {
        return Err(fail("IR contains unreachable blocks"));
    }
    // Dense block IDs permit word-wise intersections. Charge actual word work
    // rather than repeatedly allocating/hashing one entry per possible dominator.
    let words = n.div_ceil(64);
    verification_work(meter, n.saturating_mul(words), &function.span)?;
    let mut dominators = vec![vec![u64::MAX; words]; n];
    dominators[0].fill(0);
    dominators[0][0] = 1;
    loop {
        let mut changed = false;
        for block in 1..n {
            verification_work(meter, words, &function.span)?;
            let mut next = vec![u64::MAX; words];
            for pred in &predecessors[block] {
                verification_work(meter, words, &function.span)?;
                for (word, parent) in next.iter_mut().zip(&dominators[*pred]) {
                    *word &= parent;
                }
            }
            next[block / 64] |= 1 << (block % 64);
            verification_work(meter, words, &function.span)?;
            if next != dominators[block] {
                dominators[block] = next;
                changed = true;
            }
        }
        if !changed { break; }
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
        if dominators[block][owner / 64] & (1 << (owner % 64)) == 0
            || (*owner == block && *definition_position >= position)
        {
            return Err(fail("IR definition does not dominate its use"));
        }
        Ok(())
    };
    for (block_id, block) in function.blocks.iter().enumerate() {
        for (index, inst) in block.instructions.iter().enumerate() {
            verification_work(meter, 1, &inst.span)?;
            for value in operands(&inst.operation) {
                verification_work(meter, 1, &inst.span)?;
                check_use(*value, block_id, block.parameters.len() + index)?;
            }
            let result_ty = ty(inst.result)?;
            match &inst.operation {
                Operation::RegionPush { handler, cleanup, end, dynamic } => {
                    if result_ty != Type::Bool || dynamic.len() > 1
                        || handler.iter().chain(cleanup.iter()).chain(std::iter::once(end)).any(|id| *id >= n || !function.blocks[*id].parameters.is_empty()) {
                        return Err(fail("IR resumable region shape mismatch"));
                    }
                }
                Operation::RegionExit { .. } | Operation::RegionValue => {
                    if result_ty != Type::Value { return Err(fail("IR region outcome must be dynamic Value")); }
                }
                Operation::AsyncDynamicEnter { globals, operands } => {
                    if result_ty != Type::Value || operands.len() != globals.len() * 2 {
                        return Err(fail("IR async dynamic entry shape mismatch"));
                    }
                }
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
                    if body.variadic { return Err(fail("IR fixed closure cannot have variadic entry")); }
                    if body.display_name.as_ref().is_some_and(|name| name.len() > i32::MAX as usize)
                        || body.arity > i32::MAX as usize
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
                    verification_work(meter, expected, &inst.span)?;
                    let mut entry = body.capture_types.clone();
                    entry.extend(std::iter::repeat_n(Type::Value, body.arity));
                    verify_function(&body.function, &entry, meter)?;
                }
                Operation::MakeFuture { body, captures } => {
                    if result_ty != Type::Value || body.capture_types.len() != captures.len()
                        || !body.continuation.function.blocks.first().is_some_and(|block| block.parameters.len() == captures.len()) {
                        return Err(fail("IR future capture shape mismatch"));
                    }
                    for (value, expected) in captures.iter().zip(&body.capture_types) {
                        if ty(*value)? != *expected { return Err(fail("IR future capture type mismatch")); }
                    }
                    verify_function(&body.continuation.function, &body.capture_types, meter)?;
                    r#async::verify_metered(&body.continuation, meter)?;
                }
                Operation::MakeGeneralClosure { body, captures } => {
                    if body.display_name.as_ref().is_some_and(|name| name.len() > i32::MAX as usize) || body.methods.is_empty()
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
                    verification_work(meter, body.capture_types.len().saturating_add(body.methods.len()), &inst.span)?;
                    let mut environment = body.capture_types.clone();
                    if body.self_capture {
                        environment.push(Type::Value);
                    }
                    let mut arities = HashSet::new();
                    let variadic: Vec<_> = body.methods.iter().filter(|method| method.variadic).collect();
                    if variadic.len() > 1 || body.rest_class.is_some() != (variadic.len() == 1) || variadic.first().is_some_and(|method| method.arity == 0 || body.methods.iter().any(|fixed| !fixed.variadic && fixed.arity > method.arity - 1)) {
                        return Err(fail("IR variadic closure shape mismatch"));
                    }
                    for method in &body.methods {
                        verification_work(meter, environment.len().saturating_add(1), &inst.span)?;
                        if method.arity > i32::MAX as usize
                            || method.capture_types != environment
                            || !arities.insert((method.arity, method.variadic))
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
                        verification_work(meter, expected, &inst.span)?;
                        let mut entry = environment.clone();
                        entry.extend(std::iter::repeat_n(Type::Value, method.arity));
                        verify_function(&method.function, &entry, meter)?;
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
                Operation::Await { .. } => {
                    if result_ty != Type::Value {
                        return Err(fail("IR await result must be dynamic Value"));
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
        verification_work(meter, 1, &function.span)?;
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
                verification_work(meter, arguments.len(), &function.span)?;
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
