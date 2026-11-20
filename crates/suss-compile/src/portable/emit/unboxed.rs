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
    hir::{Arithmetic, Bitwise, Comparison, Literal, Nominal},
    ir::{self, Function, Operation, Terminator, ValueId},
};
use crate::runtime_abi;
use wasm_encoder::{BlockType, HeapType, Instruction, ValType};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Repr {
    Boxed,
    Long,
    Double,
    /// A guarded typed call can produce a raw number or an arbitrary boxed
    /// replacement value. Keep both slots and a flag until the value escapes.
    MaybeLong,
    MaybeDouble,
    /// An i32 truth value, from a numeric comparison, a nil test or a
    /// boolean nominal test (#387).
    Flag,
}
impl Repr {
    pub(super) fn maybe(self) -> bool { matches!(self, Self::MaybeLong | Self::MaybeDouble) }
    pub(super) fn definite(self) -> Self {
        match self { Self::MaybeLong => Self::Long, Self::MaybeDouble => Self::Double, other => other }
    }
    fn numeric(self) -> bool {
        matches!(self, Self::Long | Self::Double)
    }
    pub(super) fn val_type(self) -> Option<ValType> {
        match self {
            Self::Boxed => None,
            Self::Long | Self::MaybeLong => Some(ValType::I64),
            Self::Double | Self::MaybeDouble => Some(ValType::F64),
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
        Comparison::Less | Comparison::NumberLess | Comparison::NumberNegative => Relation::Less,
        Comparison::LessEqual | Comparison::NumberLessEqual => Relation::LessEqual,
        Comparison::Greater | Comparison::NumberGreater | Comparison::NumberPositive => Relation::Greater,
        Comparison::GreaterEqual | Comparison::NumberGreaterEqual => Relation::GreaterEqual,
        Comparison::StrictEqual | Comparison::NumberEqual | Comparison::NumberZero => Relation::Equal,
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
        (Lattice::Known(x), Lattice::Known(y)) if x.definite() == y.definite() && (x.maybe() || y.maybe()) =>
            Lattice::Known(if x.maybe() { x } else { y }),
        _ => Lattice::Known(Repr::Boxed),
    }
}

/// The representation of an arithmetic result from its operands', optimistic
/// about operands not yet known.
fn arithmetic(operator: Arithmetic, arguments: &[Lattice], coercing: bool) -> Lattice {
    // value-* takes exactly the checked number-* route for two numbers.
    // A guarded primitive result retains the original coercing helper on a
    // boxed fallback. Unary coercions and core division stay boxed.
    if coercing && (arguments.len() != 2 || operator == Arithmetic::Divide)
    {
        return Lattice::Known(Repr::Boxed);
    }
    if arguments.iter().any(|argument| matches!(argument, Lattice::Known(repr) if repr.maybe())) {
        let definite = arguments.iter().map(|argument| match argument {
            Lattice::Known(repr) => Lattice::Known(repr.definite()), other => *other,
        }).collect::<Vec<_>>();
        return match arithmetic(operator, &definite, coercing) {
            Lattice::Known(Repr::Long) => Lattice::Known(Repr::MaybeLong),
            Lattice::Known(Repr::Double) => Lattice::Known(Repr::MaybeDouble),
            other => other,
        };
    }
    if arguments.is_empty() || arguments.len() > 2 {
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

/// Clojure's `bit-and`, `bit-or` and `bit-xor` of two longs are a long
/// (#387); anything else takes the runtime operation, which throws for a
/// double as Clojure does.
pub(super) fn raw_bitwise(operation: Bitwise, count: usize) -> Option<Instruction<'static>> {
    match operation {
        Bitwise::And if count == 2 => Some(Instruction::I64And),
        Bitwise::Or if count == 2 => Some(Instruction::I64Or),
        Bitwise::Xor if count == 2 => Some(Instruction::I64Xor),
        _ => None,
    }
}

/// Core's ToInt32 operations consume a long's low 32 bits. The result is
/// signed except for zero-fill right shift, exactly as their boxed helpers.
fn raw_int32(operation: Bitwise, count: usize) -> Option<Instruction<'static>> {
    Some(match (operation, count) {
        (Bitwise::Int32, 1) => Instruction::Nop,
        (Bitwise::BitCount, 1) => Instruction::I32Popcnt,
        (Bitwise::Int32And, 2) => Instruction::I32And,
        (Bitwise::Int32Or, 2) => Instruction::I32Or,
        (Bitwise::Int32Xor, 2) => Instruction::I32Xor,
        (Bitwise::Int32Left, 2) => Instruction::I32Shl,
        (Bitwise::Int32Right, 2) => Instruction::I32ShrS,
        (Bitwise::Int32Unsigned, 2) => Instruction::I32ShrU,
        (Bitwise::Imul, 2) => Instruction::I32Mul,
        _ => return None,
    })
}
fn bitwise(operation: Bitwise, arguments: &[Lattice]) -> Lattice {
    // ToInt32 produces a long or throws even when its input is boxed. Keep
    // that result fact without bypassing the boxed operand conversions.
    if raw_int32(operation, arguments.len()).is_some() {
        return Lattice::Known(Repr::Long);
    }
    let test = operation == Bitwise::Test && arguments.len() == 2;
    if (test || raw_bitwise(operation, arguments.len()).is_some())
        && arguments.iter().all(|argument| matches!(argument, Lattice::Top | Lattice::Known(Repr::Long)))
    {
        if arguments.iter().all(|argument| *argument == Lattice::Top) {
            Lattice::Top
        } else {
            Lattice::Known(if test { Repr::Flag } else { Repr::Long })
        }
    } else {
        Lattice::Known(Repr::Boxed)
    }
}

fn comparison(operation: Comparison, arguments: &[Lattice]) -> Lattice {
    if relation(operation).is_none() || arguments.len() != 2 {
        return Lattice::Known(Repr::Boxed);
    }
    let raw = |argument: &Lattice| matches!(argument, Lattice::Top | Lattice::Known(Repr::Long | Repr::Double | Repr::MaybeLong | Repr::MaybeDouble));
    // Core's legacy comparisons also compare two numbers numerically. Only
    // use that fact for two raw numeric operands; strings, objects and other
    // values retain the original coercing helper and its effects.
    if matches!(operation, Comparison::Less | Comparison::LessEqual
        | Comparison::Greater | Comparison::GreaterEqual | Comparison::StrictEqual)
        && !arguments.iter().all(raw)
    {
        return Lattice::Known(Repr::Boxed);
    }
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
pub(super) fn infer(function: &Function, resumable: bool, entries: &[(ValueId, Repr)], result: Option<Repr>) -> Vec<Repr> {
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
    for (value, repr) in entries { state[value.0] = Lattice::Known(*repr); }
    let mut seeded = vec![false; count];
    // A `^long` or `^double` parameter's cast is raw (#385).
    for block in &function.blocks {
        for instruction in &block.instructions {
            match &instruction.operation {
                Operation::Call { operands, primitive: Some(shape) } if operands.len() <= 9 => {
                    if let Some(signature) = shape.fixed(operands.len() - 1) {
                        state[instruction.result.0] = Lattice::Known(match signature.result {
                            super::super::primitive::Kind::Long => Repr::MaybeLong,
                            super::super::primitive::Kind::Double => Repr::MaybeDouble,
                            _ => Repr::Boxed,
                        });
                    }
                }
                Operation::Nominal { operation: super::super::hir::Nominal::LongCast | super::super::hir::Nominal::UncheckedLongCast, .. } => state[instruction.result.0] = Lattice::Known(Repr::Long),
                Operation::Nominal { operation: super::super::hir::Nominal::DoubleCast, .. } => state[instruction.result.0] = Lattice::Known(Repr::Double),
                // A hinted aget's element, behind its checked cast (#389).
                // An object array's element is a value (#378).
                Operation::Nominal { operation: super::super::hir::Nominal::PackedGet(kind), .. } if *kind != 6 => {
                    state[instruction.result.0] = Lattice::Known(if matches!(kind, 3 | 5) { Repr::Double } else { Repr::Long })
                }
                // A hash is a raw long (#387).
                Operation::Nominal { operation: super::super::hir::Nominal::NumberHash, .. } => state[instruction.result.0] = Lattice::Known(Repr::Long),
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
                    | Operation::Bitwise { .. }
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
                        Operation::Bitwise { operation, arguments } => bitwise(*operation, &operands(arguments)),
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
            if let Operation::Call { operands, primitive: Some(shape) } = &instruction.operation
                && operands.len() <= 9
                && let Some(signature) = shape.fixed(operands.len() - 1)
            {
                for (value, kind) in operands[1..].iter().zip(&signature.parameters) {
                    if *kind != super::super::primitive::Kind::Boxed && repr[value.0].definite().numeric() {
                        raw_use[value.0] = true;
                    }
                }
            }
            if let Operation::Nominal { operation: Nominal::LongCast | Nominal::UncheckedLongCast | Nominal::DoubleCast, arguments } = &instruction.operation
                && repr[arguments[0].0].definite().numeric()
            {
                raw_use[arguments[0].0] = true;
            }
            let boxed_int32 = matches!(&instruction.operation,
                Operation::Bitwise { operation, arguments }
                if raw_int32(*operation, arguments.len()).is_some()
                    && arguments.iter().any(|argument| repr[argument.0] != Repr::Long));
            if repr[instruction.result.0] != Repr::Boxed && !boxed_int32
                && matches!(
                    instruction.operation,
                    Operation::Arithmetic { .. } | Operation::Comparison { .. } | Operation::Bitwise { .. }
                )
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
        if let Terminator::Return(value) = &block.terminator
            && result == Some(repr[value.0])
        {
            raw_use[value.0] = true;
        }
        // A hinted array access reads a long index and stores a value of its
        // element's kind raw (#389).
        for instruction in &block.instructions {
            if let Operation::Nominal { operation: Nominal::PackedGet(kind) | Nominal::PackedSet(kind), arguments } =
                &instruction.operation
            {
                if repr[arguments[1].0] == Repr::Long {
                    raw_use[arguments[1].0] = true;
                }
                if let Some(value) = arguments.get(2)
                    && Some(repr[value.0]) == stored(*kind)
                {
                    raw_use[value.0] = true;
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
    // A hinted aset returns its raw value raw (#389).
    for block in &function.blocks {
        for instruction in &block.instructions {
            if let Operation::Nominal { operation: Nominal::PackedSet(kind), arguments } = &instruction.operation
                && Some(repr[arguments[2].0]) == stored(*kind)
            {
                repr[instruction.result.0] = repr[arguments[2].0];
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

/// The raw kind a hinted aset of a packed `kind` stores inline (#389): a
/// long into a long or int array, a double into a double array.
pub(super) fn stored(kind: usize) -> Option<Repr> {
    match kind {
        2 | 4 => Some(Repr::Long),
        5 => Some(Repr::Double),
        _ => None,
    }
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
    valid: Vec<Option<u32>>,
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
        for kind in [Repr::Long, Repr::Double, Repr::Flag, Repr::MaybeLong, Repr::MaybeDouble] {
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
        let mut valid = vec![None; repr.len()];
        let start = next;
        for (value, item) in repr.iter().enumerate() {
            if item.maybe() { valid[value] = Some(next); next += 1; }
        }
        if next > start { declarations.push((next - start, ValType::I32)); }
        Self { repr, raw, valid, declarations, offset, sentinels }
    }
    pub(super) fn of(&self, value: ValueId) -> Repr {
        self.repr[value.0]
    }
    pub(super) fn offset(&self) -> u32 { self.offset }
    pub(super) fn raw(&self, value: ValueId) -> u32 {
        self.raw[value.0].expect("raw local")
    }
    pub(super) fn valid(&self, value: ValueId) -> u32 { self.valid[value.0].expect("guarded numeric local") }
    fn boxed(&self, value: ValueId) -> u32 {
        value.0 as u32 + self.offset
    }
    /// Pushes `value` as a GC value, boxing a raw one.
    pub(super) fn push_boxed(&self, code: &mut Vec<Instruction<'static>>, value: ValueId) {
        self.push_kind_boxed(code, value, self.of(value));
    }
    fn push_kind_boxed(&self, code: &mut Vec<Instruction<'static>>, value: ValueId, kind: Repr) {
        use Instruction::*;
        match kind {
            Repr::MaybeLong | Repr::MaybeDouble => {
                code.extend([LocalGet(self.valid(value)), If(BlockType::Result(VALUE))]);
                self.push_kind_boxed(code, value, kind.definite());
                code.extend([Else, LocalGet(self.boxed(value)), End]);
            }
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
        if kind == Repr::Double && self.of(value).definite() == Repr::Long {
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
        if kind.maybe() {
            let inputs = arguments.iter().filter(|value| self.of(**value).maybe()).collect::<Vec<_>>();
            for (i, input) in inputs.iter().enumerate() {
                code.push(LocalGet(self.valid(**input)));
                if i > 0 { code.push(I32And); }
            }
            code.push(If(BlockType::Empty));
            self.arithmetic_raw(code, operator, arguments, result, slow);
            code.extend([I32Const(1), LocalSet(self.valid(result)), Else]);
            for argument in arguments { self.push_boxed(code, *argument); }
            code.extend([Call(slow), LocalSet(self.boxed(result)), I32Const(0), LocalSet(self.valid(result)), End]);
            return;
        }
        self.arithmetic_raw(code, operator, arguments, result, slow);
    }
    fn arithmetic_raw(&self, code: &mut Vec<Instruction<'static>>, operator: Arithmetic, arguments: &[ValueId], result: ValueId, slow: u32) {
        use Instruction::*;
        let kind = self.of(result).definite();
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
        let inputs = arguments.iter().filter(|value| self.of(**value).maybe()).collect::<Vec<_>>();
        if !inputs.is_empty() {
            for (i, input) in inputs.iter().enumerate() {
                code.push(LocalGet(self.valid(**input)));
                if i > 0 { code.push(I32And); }
            }
            code.push(If(BlockType::Empty));
            self.comparison_raw(code, operation, arguments, result, slow);
            code.push(Else);
            for argument in arguments { self.push_boxed(code, *argument); }
            code.extend([Call(slow), runtime_abi::sentinel_in(runtime_abi::TRUE, self.sentinels),
                RefEq, LocalSet(self.raw(result)), End]);
            return;
        }
        self.comparison_raw(code, operation, arguments, result, slow);
    }
    fn comparison_raw(
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
        let (rx, ry) = (self.of(x).definite(), self.of(y).definite());
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

    /// `identical?` with a raw operand (#387), without boxing it. A long is
    /// identical to a long of the same value, an i31 or a long box, and a
    /// double to a boxed double that compares equal, as `identical-value`
    /// answers for two boxed numbers; a long is never a double.
    pub(super) fn identical(&self, code: &mut Vec<Instruction<'static>>, arguments: &[ValueId], result: ValueId) {
        use Instruction::*;
        let (x, y) = (arguments[0], arguments[1]);
        let (rx, ry) = (self.of(x), self.of(y));
        match (rx, ry) {
            (Repr::Long, Repr::Long) => code.extend([LocalGet(self.raw(x)), LocalGet(self.raw(y)), I64Eq]),
            (Repr::Double, Repr::Double) => code.extend([LocalGet(self.raw(x)), LocalGet(self.raw(y)), F64Eq]),
            (Repr::Long, Repr::Boxed) | (Repr::Boxed, Repr::Long) => {
                let (raw, other) = if rx == Repr::Long { (x, y) } else { (y, x) };
                let (raw, other) = (self.raw(raw), self.boxed(other));
                code.extend([
                    LocalGet(other), RefTestNonNull(HeapType::I31),
                    If(BlockType::Result(ValType::I32)),
                    LocalGet(other), RefCastNonNull(HeapType::I31), I31GetS, I64ExtendI32S, LocalGet(raw), I64Eq,
                    Else,
                    LocalGet(other), RefTestNonNull(HeapType::Concrete(runtime_abi::LONG)),
                    If(BlockType::Result(ValType::I32)),
                    LocalGet(other), RefCastNonNull(HeapType::Concrete(runtime_abi::LONG)),
                    StructGet { struct_type_index: runtime_abi::LONG, field_index: 0 }, LocalGet(raw), I64Eq,
                    Else, I32Const(0), End,
                    End,
                ]);
            }
            (Repr::Double, Repr::Boxed) | (Repr::Boxed, Repr::Double) => {
                let (raw, other) = if rx == Repr::Double { (x, y) } else { (y, x) };
                let (raw, other) = (self.raw(raw), self.boxed(other));
                code.extend([
                    LocalGet(other), RefTestNonNull(HeapType::Concrete(runtime_abi::NUMBER)),
                    If(BlockType::Result(ValType::I32)),
                    LocalGet(other), RefCastNonNull(HeapType::Concrete(runtime_abi::NUMBER)),
                    StructGet { struct_type_index: runtime_abi::NUMBER, field_index: 0 }, LocalGet(raw), F64Eq,
                    Else, I32Const(0), End,
                ]);
            }
            // A long and a double, raw.
            _ => code.push(I32Const(0)),
        }
        if self.of(result) == Repr::Flag {
            code.push(LocalSet(self.raw(result)));
        } else {
            code.extend(runtime_abi::boolean_in(self.sentinels));
            code.push(LocalSet(self.boxed(result)));
        }
    }

    /// Pushes whether a raw long fits an i31.
    fn push_fits(&self, code: &mut Vec<Instruction<'static>>, value: ValueId) {
        use Instruction::*;
        code.extend([LocalGet(self.raw(value)), I64Const(1 << 30), I64Add, I64Const(1 << 31), I64LtU]);
    }
    /// A small-integer or same-reference test of raw longs and GC values
    /// (`raw_small`). A raw long is an i31 exactly when it fits one, and two
    /// i31s are the same reference exactly when equal; a long outside i31
    /// would box afresh, the same reference as nothing.
    pub(super) fn small(&self, code: &mut Vec<Instruction<'static>>, same: bool, arguments: &[ValueId], result: ValueId) {
        use Instruction::*;
        if same {
            let (x, y) = (arguments[0], arguments[1]);
            if self.of(x) == Repr::Long && self.of(y) == Repr::Long {
                self.push_fits(code, x);
                code.extend([LocalGet(self.raw(x)), LocalGet(self.raw(y)), I64Eq, I32And]);
            } else {
                let (raw, other) = if self.of(x) == Repr::Long { (x, y) } else { (y, x) };
                let other = self.boxed(other);
                code.extend([
                    LocalGet(other), RefTestNonNull(HeapType::I31),
                    If(BlockType::Result(ValType::I32)),
                    LocalGet(other), RefCastNonNull(HeapType::I31), I31GetS, I64ExtendI32S, LocalGet(self.raw(raw)), I64Eq,
                    Else, I32Const(0), End,
                ]);
            }
        } else {
            for (k, argument) in arguments.iter().enumerate() {
                if self.of(*argument) == Repr::Long {
                    self.push_fits(code, *argument);
                } else {
                    code.extend([LocalGet(self.boxed(*argument)), RefTestNonNull(HeapType::I31)]);
                }
                if k > 0 {
                    code.push(I32And);
                }
            }
        }
        if self.of(result) == Repr::Flag {
            code.push(LocalSet(self.raw(result)));
        } else {
            code.extend(runtime_abi::boolean_in(self.sentinels));
            code.push(LocalSet(self.boxed(result)));
        }
    }
    /// A raw long's hash into the raw result, with `h` as scratch.
    pub(super) fn hash(&self, code: &mut Vec<Instruction<'static>>, argument: ValueId, result: ValueId, h: u32) {
        code.extend(runtime_abi::hash_long_in(self.raw(argument), h));
        code.extend([Instruction::I64ExtendI32S, Instruction::LocalSet(self.raw(result))]);
    }

    /// A raw long bitwise operation, which can't fail. bit-test produces a
    /// flag; Wasm masks the shift count to six bits, as Java's long shift does.
    pub(super) fn bitwise(&self, code: &mut Vec<Instruction<'static>>, operation: Bitwise, arguments: &[ValueId], result: ValueId) {
        if let Some(operation_code) = raw_int32(operation, arguments.len()) {
            for argument in arguments {
                code.extend([Instruction::LocalGet(self.raw(*argument)), Instruction::I32WrapI64]);
            }
            code.push(operation_code);
            code.push(if operation == Bitwise::Int32Unsigned {
                Instruction::I64ExtendI32U
            } else {
                Instruction::I64ExtendI32S
            });
            code.push(Instruction::LocalSet(self.raw(result)));
            return;
        }
        code.extend([
            Instruction::LocalGet(self.raw(arguments[0])),
            Instruction::LocalGet(self.raw(arguments[1])),
        ]);
        if operation == Bitwise::Test {
            code.extend([Instruction::I64ShrU, Instruction::I64Const(1),
                Instruction::I64And, Instruction::I32WrapI64]);
        } else {
            code.push(raw_bitwise(operation, arguments.len()).expect("raw bitwise"));
        }
        code.push(Instruction::LocalSet(self.raw(result)));
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

/// Whether an instruction is an `identical?` of a raw number (#387), which
/// `Locals::identical` emits without boxing it.
pub(super) fn raw_identical(locals: &Locals, instruction: &ir::Instruction) -> bool {
    if ir::operands(&instruction.operation).iter().any(|value| locals.of(*value).maybe()) { return false; }
    match &instruction.operation {
        Operation::Nominal { operation: super::super::hir::Nominal::Identical, arguments } => {
            arguments.len() == 2 && arguments.iter().any(|argument| locals.of(*argument).numeric())
        }
        _ => false,
    }
}

/// Whether an instruction is a small-integer test, a same-reference test or
/// a hash with a raw long operand and no raw double (#387), which emission
/// reads raw rather than boxing the operand.
pub(super) fn raw_small(locals: &Locals, instruction: &ir::Instruction) -> bool {
    if ir::operands(&instruction.operation).iter().any(|value| locals.of(*value).maybe()) { return false; }
    use super::super::hir::Nominal;
    let Operation::Nominal { operation, arguments } = &instruction.operation else { return false };
    let long = arguments.iter().any(|argument| locals.of(*argument) == Repr::Long);
    let double = arguments.iter().any(|argument| locals.of(*argument) == Repr::Double);
    long && !double
        && match operation {
            Nominal::AllSmallIntegers => true,
            Nominal::SameReference => arguments.len() == 2,
            Nominal::NumberHash => locals.of(instruction.result) == Repr::Long,
            _ => false,
        }
}

/// Whether an instruction is emitted inline on raw values.
pub(super) fn inline(locals: &Locals, instruction: &ir::Instruction) -> bool {
    if let Operation::Bitwise { operation, arguments } = &instruction.operation
        && raw_int32(*operation, arguments.len()).is_some()
        && arguments.iter().any(|argument| locals.of(*argument) != Repr::Long)
    {
        return false;
    }
    locals.of(instruction.result) != Repr::Boxed
        && matches!(
            instruction.operation,
            Operation::Literal(_)
                | Operation::Arithmetic { .. }
                | Operation::Comparison { .. }
                | Operation::Bitwise { .. }
        )
}
