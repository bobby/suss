//! Explicit values, blocks and edge parameters. Verification precedes emission.
use super::{
    Diagnostic,
    hir::{Arithmetic, BindingId, Expression, Hir, Literal, Type},
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
    Literal(Literal),
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
struct Lowerer {
    function: Function,
    current: usize,
    bindings: HashMap<BindingId, ValueId>,
}
impl Lowerer {
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
    fn expression(&mut self, hir: &Hir) -> Result<ValueId, Diagnostic> {
        Ok(match &hir.kind {
            Expression::Literal(value) => self.literal(value.clone(), hir.span.clone()),
            Expression::Local(id) => *self.bindings.get(id).ok_or_else(|| Diagnostic {
                span: hir.span.clone(),
                message: "HIR references an undefined binding".into(),
            })?,
            Expression::Do(items) => {
                let mut result = None;
                for item in items {
                    result = Some(self.expression(item)?);
                }
                result.unwrap_or_else(|| self.literal(Literal::Nil, hir.span.clone()))
            }
            Expression::Let { bindings, body } => {
                // Binding identities, rather than names, distinguish shadowed values.
                for binding in bindings {
                    let value = self.expression(&binding.value)?;
                    if self.bindings.insert(binding.id, value).is_some() {
                        return Err(Diagnostic {
                            span: binding.span.clone(),
                            message: "HIR binding identity is defined twice".into(),
                        });
                    }
                }
                self.expression(body)?
            }
            Expression::If {
                condition,
                consequent,
                alternative,
            } => {
                let condition = self.expression(condition)?;
                let result = self.value(hir.ty, hir.span.clone());
                let then_block = self.block(Vec::new());
                let else_block = self.block(Vec::new());
                let join = self.block(vec![result]);
                self.function.blocks[self.current].terminator = Terminator::Branch {
                    condition,
                    consequent: then_block,
                    alternative: else_block,
                };
                self.current = then_block;
                let value = self.expression(consequent)?;
                self.function.blocks[self.current].terminator = Terminator::Jump {
                    target: join,
                    arguments: vec![value],
                };
                self.current = else_block;
                let value = self.expression(alternative)?;
                self.function.blocks[self.current].terminator = Terminator::Jump {
                    target: join,
                    arguments: vec![value],
                };
                self.current = join;
                result
            }
            Expression::Arithmetic {
                operator,
                arguments,
            } => {
                // Evaluate every operand before entering the arithmetic operation.
                let values = arguments
                    .iter()
                    .map(|arg| self.expression(arg))
                    .collect::<Result<Vec<_>, _>>()?;
                if values.is_empty() {
                    return Ok(self.literal(
                        Literal::Number(if *operator == Arithmetic::Add {
                            0.0
                        } else {
                            1.0
                        }),
                        hir.span.clone(),
                    ));
                }
                let mut result = values[0];
                if values.len() == 1 {
                    if *operator == Arithmetic::Subtract {
                        return Ok(self.emit(
                            Operation::Arithmetic {
                                operator: Arithmetic::Negate,
                                arguments: values,
                            },
                            Type::Number,
                            hir.span.clone(),
                        ));
                    }
                    if *operator == Arithmetic::Divide {
                        let one = self.literal(Literal::Number(1.0), hir.span.clone());
                        return Ok(self.emit(
                            Operation::Arithmetic {
                                operator: *operator,
                                arguments: vec![one, result],
                            },
                            Type::Number,
                            hir.span.clone(),
                        ));
                    }
                }
                for value in &values[1..] {
                    result = self.emit(
                        Operation::Arithmetic {
                            operator: *operator,
                            arguments: vec![result, *value],
                        },
                        Type::Number,
                        hir.span.clone(),
                    );
                }
                result
            }
        })
    }
}
pub fn lower(hir: &Hir) -> Result<Function, Diagnostic> {
    let mut lowerer = Lowerer {
        function: Function {
            values: Vec::new(),
            blocks: Vec::new(),
            span: hir.span.clone(),
        },
        current: 0,
        bindings: HashMap::new(),
    };
    lowerer.block(Vec::new());
    let result = lowerer.expression(hir)?;
    lowerer.function.blocks[lowerer.current].terminator = Terminator::Return(result);
    Ok(lowerer.function)
}
fn operands(operation: &Operation) -> &[ValueId] {
    match operation {
        Operation::Literal(_) => &[],
        Operation::Arithmetic { arguments, .. } => arguments,
    }
}
/// Check graph integrity, unique definitions, dominance, edge arities and types.
/// All supported calls are resolved Number intrinsics; no unknown effect is emitted.
pub fn verify(function: &Function) -> Result<(), Diagnostic> {
    let fail = |message: &str| Diagnostic {
        span: function.span.clone(),
        message: message.into(),
    };
    let n = function.blocks.len();
    if n == 0 || !function.blocks[0].parameters.is_empty() {
        return Err(fail("IR requires a parameterless entry block"));
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
            Terminator::Return(_) => Vec::new(),
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
                Operation::Literal(value) if result_ty != value.ty() => {
                    return Err(fail("IR literal result type mismatch"));
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
                    if result_ty != Type::Number
                        || arguments
                            .iter()
                            .any(|arg| !matches!(ty(*arg), Ok(Type::Number)))
                    {
                        return Err(fail("IR intrinsic requires Number values"));
                    }
                }
                _ => {}
            }
        }
        let position = block.parameters.len() + block.instructions.len();
        match &block.terminator {
            Terminator::Return(value) => check_use(*value, block_id, position)?,
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
