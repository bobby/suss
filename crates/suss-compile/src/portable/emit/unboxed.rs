//! Unboxed numeric locals (#337, ADR-0003 "Representation").
//!
//! Every IR value has a GC local. A value the compiler knows to be a long, a
//! double or the boolean result of a numeric comparison also gets a raw `i64`,
//! `f64` or `i32` local, which arithmetic, comparisons, branches and loop
//! parameters use directly. Any other use boxes the raw value at that use, so a
//! number allocates only where it escapes: into a collection, a closure, a
//! call or a return. Overflow and other errors take the boxed runtime path, so
//! messages and exceptions match it exactly.
use super::super::{
    hir::{Arithmetic, Comparison, Literal},
    ir::{self, Function, Operation, Terminator, ValueId},
};
use crate::runtime_abi;
use wasm_encoder::{BlockType, HeapType, Instruction, ValType};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Repr {
    Boxed,
    Long,
    Double,
    /// An i32 truth value, from a numeric comparison, a nil test or a
    /// boolean nominal test (#387).
    Flag,
}
impl Repr {
    fn numeric(self) -> bool {
        matches!(self, Self::Long | Self::Double)
    }
    fn val_type(self) -> Option<ValType> {
        match self {
            Self::Boxed => None,
            Self::Long => Some(ValType::I64),
            Self::Double => Some(ValType::F64),
            Self::Flag => Some(ValType::I32),
        }
    }
}

/// A numeric relation the compiler emits inline, with its operands' order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Relation {
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    Equal,
}
pub(super) fn relation(operation: Comparison) -> Option<Relation> {
    Some(match operation {
        Comparison::NumberLess | Comparison::NumberNegative => Relation::Less,
        Comparison::NumberLessEqual => Relation::LessEqual,
        Comparison::NumberGreater | Comparison::NumberPositive => Relation::Greater,
        Comparison::NumberGreaterEqual => Relation::GreaterEqual,
        Comparison::NumberEqual | Comparison::NumberZero => Relation::Equal,
        _ => return None,
    })
}

/// The lattice is Top (unknown yet) above the raw kinds, above Boxed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Lattice {
    Top,
    Known(Repr),
}
fn meet(a: Lattice, b: Lattice) -> Lattice {
    match (a, b) {
        (Lattice::Top, other) | (other, Lattice::Top) => other,
        (Lattice::Known(x), Lattice::Known(y)) if x == y => a,
        _ => Lattice::Known(Repr::Boxed),
    }
}

/// The representation of an arithmetic result from its operands', optimistic
/// about operands not yet known.
fn arithmetic(operator: Arithmetic, arguments: &[Lattice], coercing: bool) -> Lattice {
    // Shipped core keeps its coercing JavaScript arithmetic boxed.
    if coercing || arguments.is_empty() || arguments.len() > 2 {
        return Lattice::Known(Repr::Boxed);
    }
    if arguments.len() == 1 && !matches!(operator, Arithmetic::Negate) {
        // Unary + and * check their operand and return it.
        return match arguments[0] {
            Lattice::Known(repr) if !repr.numeric() => Lattice::Known(Repr::Boxed),
            other => other,
        };
    }
    let mut double = false;
    for argument in arguments {
        match argument {
            Lattice::Top => {}
            Lattice::Known(Repr::Double) => double = true,
            Lattice::Known(Repr::Long) => {}
            Lattice::Known(_) => return Lattice::Known(Repr::Boxed),
        }
    }
    if double {
        Lattice::Known(Repr::Double)
    } else if arguments.iter().all(|argument| *argument == Lattice::Top)
        || (operator == Arithmetic::Divide && arguments.contains(&Lattice::Top))
    {
        // A quotient waits for every operand: one double makes it raw.
        Lattice::Top
    } else if operator == Arithmetic::Divide {
        // A quotient of longs is a long only when exact.
        Lattice::Known(Repr::Boxed)
    } else {
        Lattice::Known(Repr::Long)
    }
}

fn comparison(operation: Comparison, arguments: &[Lattice]) -> Lattice {
    if relation(operation).is_none() || arguments.len() != 2 {
        return Lattice::Known(Repr::Boxed);
    }
    let raw = |argument: &Lattice| matches!(argument, Lattice::Top | Lattice::Known(Repr::Long | Repr::Double));
    let long = |argument: &Lattice| matches!(argument, Lattice::Top | Lattice::Known(Repr::Long));
    let boxed = |argument: &Lattice| *argument == Lattice::Known(Repr::Boxed);
    // Two numbers, or a long and a value that may be a small long.
    if arguments.iter().all(raw)
        || (long(&arguments[0]) && boxed(&arguments[1]))
        || (boxed(&arguments[0]) && long(&arguments[1]))
    {
        Lattice::Known(Repr::Flag)
    } else {
        Lattice::Known(Repr::Boxed)
    }
}

/// Each value's representation. Resumable functions, whose locals are spilled
/// as GC values across suspensions, keep every value boxed.
pub(super) fn infer(function: &Function, resumable: bool) -> Vec<Repr> {
    let count = function.values.len();
    if resumable {
        return vec![Repr::Boxed; count];
    }
    // Block parameters take their incoming arguments' meet; a function's own
    // parameters arrive boxed.
    let mut incoming: Vec<Vec<ValueId>> = vec![Vec::new(); count];
    let mut parameter = vec![false; count];
    for (index, block) in function.blocks.iter().enumerate() {
        for value in &block.parameters {
            parameter[value.0] = index != 0;
        }
        if let Terminator::Jump { target, arguments } = &block.terminator {
            for (value, argument) in function.blocks[*target].parameters.iter().zip(arguments) {
                incoming[value.0].push(*argument);
            }
        }
    }
    let mut state = vec![Lattice::Known(Repr::Boxed); count];
    let mut seeded = vec![false; count];
    // A `^long` or `^double` parameter's cast is raw (#385).
    for block in &function.blocks {
        for instruction in &block.instructions {
            match &instruction.operation {
                Operation::Nominal { operation: super::super::hir::Nominal::LongCast, .. } => state[instruction.result.0] = Lattice::Known(Repr::Long),
                Operation::Nominal { operation: super::super::hir::Nominal::DoubleCast, .. } => state[instruction.result.0] = Lattice::Known(Repr::Double),
                _ => {}
            }
        }
    }
    for block in &function.blocks {
        for instruction in &block.instructions {
            if matches!(
                instruction.operation,
                Operation::Literal(Literal::Long(_) | Literal::Number(_))
                    | Operation::Arithmetic { .. }
                    | Operation::Comparison { .. }
            ) {
                state[instruction.result.0] = Lattice::Top;
                seeded[instruction.result.0] = true;
            }
        }
        for value in &block.parameters {
            if parameter[value.0] && !incoming[value.0].is_empty() {
                state[value.0] = Lattice::Top;
                seeded[value.0] = true;
            }
        }
    }
    loop {
        loop {
            let mut step = false;
            for block in &function.blocks {
                for value in &block.parameters {
                    if !seeded[value.0] {
                        continue;
                    }
                    let next = incoming[value.0]
                        .iter()
                        .fold(Lattice::Top, |all, argument| meet(all, state[argument.0]));
                    if next != state[value.0] {
                        state[value.0] = next;
                        step = true;
                    }
                }
                for instruction in &block.instructions {
                    if !seeded[instruction.result.0] {
                        continue;
                    }
                    let operands = |arguments: &[ValueId]| {
                        arguments.iter().map(|argument| state[argument.0]).collect::<Vec<_>>()
                    };
                    let next = match &instruction.operation {
                        Operation::Literal(Literal::Long(_)) => Lattice::Known(Repr::Long),
                        Operation::Literal(_) => Lattice::Known(Repr::Double),
                        Operation::Arithmetic { operator, arguments, coercing } => {
                            arithmetic(*operator, &operands(arguments), *coercing)
                        }
                        Operation::Comparison { operation, arguments } => {
                            comparison(*operation, &operands(arguments))
                        }
                        _ => unreachable!("seeded numeric instruction"),
                    };
                    if next != state[instruction.result.0] {
                        state[instruction.result.0] = next;
                        step = true;
                    }
                }
            }
            if !step {
                break;
            }
        }
        // A cycle of parameters with no numeric source is unreachable or never
        // numeric: box it and settle the rest again.
        let mut settled = true;
        for value in 0..count {
            if state[value] == Lattice::Top {
                state[value] = Lattice::Known(Repr::Boxed);
                seeded[value] = false;
                settled = false;
            }
        }
        if settled {
            break;
        }
    }
    let mut repr = state
        .into_iter()
        .map(|lattice| match lattice {
            Lattice::Known(repr) => repr,
            Lattice::Top => Repr::Boxed,
        })
        .collect::<Vec<_>>();
    // A literal that only ever escapes stays a boxed constant. Its users are
    // then not inline, so no other value's representation depends on it.
    let mut raw_use = vec![false; count];
    for block in &function.blocks {
        for instruction in &block.instructions {
            if repr[instruction.result.0] != Repr::Boxed
                && matches!(instruction.operation, Operation::Arithmetic { .. } | Operation::Comparison { .. })
            {
                for argument in ir::operands(&instruction.operation) {
                    raw_use[argument.0] = true;
                }
            }
        }
        if let Terminator::Jump { target, arguments } = &block.terminator {
            for (value, argument) in function.blocks[*target].parameters.iter().zip(arguments) {
                if repr[value.0] != Repr::Boxed {
                    raw_use[argument.0] = true;
                }
            }
        }
    }
    for block in &function.blocks {
        for instruction in &block.instructions {
            if matches!(instruction.operation, Operation::Literal(_)) && !raw_use[instruction.result.0] {
                repr[instruction.result.0] = Repr::Boxed;
            }
        }
    }
    // A nil test or a boolean nominal test keeps its i32 truth value (#387):
    // a branch tests it directly, and any other use takes the boolean
    // sentinel, as a numeric comparison's flag does.
    for block in &function.blocks {
        for instruction in &block.instructions {
            if flag_test(&instruction.operation) {
                repr[instruction.result.0] = Repr::Flag;
            }
        }
    }
    repr
}

/// Whether the operation computes an i32 truth value that emission boxes.
pub(super) fn flag_test(operation: &Operation) -> bool {
    match operation {
        Operation::NilTest(_) => true,
        Operation::Nominal { operation, .. } => operation.result() == super::super::hir::Type::Bool,
        _ => false,
    }
}

/// Raw locals for the unboxed values, numbered from `base`.
pub(super) struct Locals {
    pub repr: Vec<Repr>,
    raw: Vec<Option<u32>>,
    pub declarations: Vec<(u32, ValType)>,
    /// GC locals are `value + offset`.
    offset: u32,
    sentinels: u32,
}
impl Locals {
    pub(super) fn new(repr: Vec<Repr>, base: u32, offset: u32, sentinels: u32) -> Self {
        let mut raw = vec![None; repr.len()];
        let mut declarations = Vec::new();
        let mut next = base;
        for kind in [Repr::Long, Repr::Double, Repr::Flag] {
            let mut count = 0;
            for (value, item) in repr.iter().enumerate() {
                if *item == kind {
                    raw[value] = Some(next);
                    next += 1;
                    count += 1;
                }
            }
            if count > 0 {
                declarations.push((count, kind.val_type().expect("raw kind")));
            }
        }
        Self { repr, raw, declarations, offset, sentinels }
    }
    pub(super) fn of(&self, value: ValueId) -> Repr {
        self.repr[value.0]
    }
    pub(super) fn raw(&self, value: ValueId) -> u32 {
        self.raw[value.0].expect("raw local")
    }
    fn boxed(&self, value: ValueId) -> u32 {
        value.0 as u32 + self.offset
    }
    /// Pushes `value` as a GC value, boxing a raw one.
    pub(super) fn push_boxed(&self, code: &mut Vec<Instruction<'static>>, value: ValueId) {
        use Instruction::*;
        match self.of(value) {
            Repr::Boxed => code.push(LocalGet(self.boxed(value))),
            Repr::Long => {
                let raw = self.raw(value);
                code.extend([
                    LocalGet(raw),
                    I64Const(1 << 30),
                    I64Add,
                    I64Const(1 << 31),
                    I64LtU,
                    If(BlockType::Result(VALUE)),
                    LocalGet(raw),
                    I32WrapI64,
                    RefI31,
                    Else,
                    LocalGet(raw),
                    StructNew(runtime_abi::LONG),
                    End,
                ]);
            }
            Repr::Double => code.extend([LocalGet(self.raw(value)), StructNew(runtime_abi::NUMBER)]),
            Repr::Flag => {
                code.push(LocalGet(self.raw(value)));
                code.extend(runtime_abi::boolean_in(self.sentinels));
            }
        }
    }
    /// Stores a GC copy of each raw operand in its GC local, once per block.
    pub(super) fn materialize(
        &self,
        code: &mut Vec<Instruction<'static>>,
        operands: &[ValueId],
        done: &mut std::collections::HashSet<usize>,
    ) {
        for value in operands {
            if self.of(*value) != Repr::Boxed && done.insert(value.0) {
                self.push_boxed(code, *value);
                code.push(Instruction::LocalSet(self.boxed(*value)));
            }
        }
    }
    /// Pushes a numeric operand as the given raw kind.
    fn push_as(&self, code: &mut Vec<Instruction<'static>>, value: ValueId, kind: Repr) {
        code.push(Instruction::LocalGet(self.raw(value)));
        if kind == Repr::Double && self.of(value) == Repr::Long {
            code.push(Instruction::F64ConvertI64S);
        }
    }

    /// Inline arithmetic whose result is raw. On long overflow the boxed
    /// runtime operation runs instead, and throws as it does.
    pub(super) fn arithmetic(
        &self,
        code: &mut Vec<Instruction<'static>>,
        operator: Arithmetic,
        arguments: &[ValueId],
        result: ValueId,
        slow: u32,
    ) {
        use Instruction::*;
        let kind = self.of(result);
        let target = self.raw(result);
        if arguments.len() == 1 && operator != Arithmetic::Negate {
            code.extend([LocalGet(self.raw(arguments[0])), LocalSet(target)]);
            return;
        }
        if kind == Repr::Double {
            if arguments.len() == 1 {
                self.push_as(code, arguments[0], kind);
                code.push(F64Neg);
            } else {
                self.push_as(code, arguments[0], kind);
                self.push_as(code, arguments[1], kind);
                code.push(match operator {
                    Arithmetic::Add => F64Add,
                    Arithmetic::Subtract => F64Sub,
                    Arithmetic::Multiply => F64Mul,
                    Arithmetic::Divide => F64Div,
                    Arithmetic::Negate => unreachable!("binary negate"),
                });
            }
            code.push(LocalSet(target));
            return;
        }
        if arguments.len() == 1 {
            // Only MIN has no negation.
            let x = self.raw(arguments[0]);
            code.extend([LocalGet(x), I64Const(i64::MIN), I64Eq, If(BlockType::Empty)]);
            self.slow(code, arguments, slow);
            code.extend([End, I64Const(0), LocalGet(x), I64Sub, LocalSet(target)]);
            return;
        }
        let (x, y) = (self.raw(arguments[0]), self.raw(arguments[1]));
        let overflow: Vec<Instruction<'static>> = match operator {
            // Signs of both operands differ from the result's.
            Arithmetic::Add => vec![
                LocalGet(x), LocalGet(x), LocalGet(y), I64Add, LocalTee(target), I64Xor,
                LocalGet(y), LocalGet(target), I64Xor, I64And, I64Const(0), I64LtS,
            ],
            Arithmetic::Subtract => vec![
                LocalGet(x), LocalGet(y), I64Xor,
                LocalGet(x), LocalGet(x), LocalGet(y), I64Sub, LocalTee(target), I64Xor,
                I64And, I64Const(0), I64LtS,
            ],
            // Division undoes an exact product; MIN * -1 is the one case where
            // that division would itself trap.
            Arithmetic::Multiply => vec![
                LocalGet(x), LocalGet(y), I64Mul, LocalSet(target),
                LocalGet(y), I64Const(-1), I64Eq,
                If(BlockType::Result(ValType::I32)),
                LocalGet(x), I64Const(i64::MIN), I64Eq,
                Else,
                LocalGet(y), I64Eqz,
                If(BlockType::Result(ValType::I32)),
                I32Const(0),
                Else,
                LocalGet(target), LocalGet(y), I64DivS, LocalGet(x), I64Ne,
                End,
                End,
            ],
            Arithmetic::Divide | Arithmetic::Negate => unreachable!("long quotient or binary negate"),
        };
        code.extend(overflow);
        code.push(If(BlockType::Empty));
        self.slow(code, arguments, slow);
        code.push(End);
    }
    /// The boxed runtime operation, which throws; it never returns here.
    fn slow(&self, code: &mut Vec<Instruction<'static>>, arguments: &[ValueId], callee: u32) {
        for argument in arguments {
            self.push_boxed(code, *argument);
        }
        code.extend([Instruction::Call(callee), Instruction::Drop, Instruction::Unreachable]);
    }

    /// An inline numeric comparison into the result's flag. A long compared
    /// with a GC value is exact when that value is an i31 and otherwise asks
    /// the runtime comparison `slow`.
    pub(super) fn comparison(
        &self,
        code: &mut Vec<Instruction<'static>>,
        operation: Comparison,
        arguments: &[ValueId],
        result: ValueId,
        slow: u32,
    ) {
        use Instruction::*;
        let relation = relation(operation).expect("inline relation");
        let target = self.raw(result);
        let (x, y) = (arguments[0], arguments[1]);
        let (rx, ry) = (self.of(x), self.of(y));
        let exact = match relation {
            Relation::Less => I64LtS,
            Relation::LessEqual => I64LeS,
            Relation::Greater => I64GtS,
            Relation::GreaterEqual => I64GeS,
            Relation::Equal => I64Eq,
        };
        if rx == Repr::Boxed || ry == Repr::Boxed {
            let other = if rx == Repr::Boxed { x } else { y };
            let other = self.boxed(other);
            code.extend([LocalGet(other), RefTestNonNull(HeapType::I31), If(BlockType::Result(ValType::I32))]);
            for (value, repr) in [(x, rx), (y, ry)] {
                if repr == Repr::Boxed {
                    code.extend([LocalGet(other), RefCastNonNull(HeapType::I31), I31GetS, I64ExtendI32S]);
                } else {
                    code.push(LocalGet(self.raw(value)));
                }
            }
            code.extend([exact, Else]);
            self.push_boxed(code, x);
            self.push_boxed(code, y);
            code.extend([Call(slow)]);
            code.extend([runtime_abi::sentinel_in(runtime_abi::TRUE, self.sentinels), RefEq, End, LocalSet(target)]);
            return;
        }
        if rx == Repr::Long && ry == Repr::Long {
            code.extend([LocalGet(self.raw(x)), LocalGet(self.raw(y)), exact, LocalSet(target)]);
            return;
        }
        self.push_as(code, x, Repr::Double);
        self.push_as(code, y, Repr::Double);
        code.extend([
            match relation {
                Relation::Less => F64Lt,
                Relation::LessEqual => F64Le,
                Relation::Greater => F64Gt,
                Relation::GreaterEqual => F64Ge,
                Relation::Equal => F64Eq,
            },
            LocalSet(target),
        ]);
    }

    pub(super) fn literal(&self, code: &mut Vec<Instruction<'static>>, literal: &Literal, result: ValueId) {
        let value = match literal {
            Literal::Long(value) => Instruction::I64Const(*value),
            Literal::Number(value) => Instruction::F64Const((*value).into()),
            _ => unreachable!("numeric literal"),
        };
        code.extend([value, Instruction::LocalSet(self.raw(result))]);
    }
}
const VALUE: ValType = ValType::Ref(wasm_encoder::RefType::EQREF);

/// Whether an instruction is emitted inline on raw values.
pub(super) fn inline(locals: &Locals, instruction: &ir::Instruction) -> bool {
    locals.of(instruction.result) != Repr::Boxed
        && matches!(
            instruction.operation,
            Operation::Literal(_) | Operation::Arithmetic { .. } | Operation::Comparison { .. }
        )
}
