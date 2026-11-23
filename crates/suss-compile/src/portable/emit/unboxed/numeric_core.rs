//! Raw forms of the existing numeric helpers. Errors take the original helper
//! with the original operands; the successful numeric path allocates nothing.
use super::*;

pub(super) fn accepts(operation: Bitwise, count: usize) -> bool {
    matches!((operation, count),
        (Bitwise::Quot | Bitwise::Rem | Bitwise::UncheckedAdd
            | Bitwise::UncheckedSubtract | Bitwise::UncheckedSubtractPrimitive | Bitwise::UncheckedMultiply, 2)
        | (Bitwise::Abs | Bitwise::UncheckedNegate, 1))
}

impl Locals {
    pub(super) fn numeric_core_raw(
        &self, code: &mut Vec<Instruction<'static>>, operation: Bitwise,
        arguments: &[ValueId], result: ValueId, slow: u32,
    ) {
        use Instruction::*;
        let kind = self.of(result).definite();
        let left = arguments[0];
        let target = self.raw(result);
        match operation {
            Bitwise::Abs if kind == Repr::Long => {
                code.extend([I64Const(0), LocalGet(self.raw(left)), I64Sub,
                    LocalGet(self.raw(left)), LocalGet(self.raw(left)), I64Const(0), I64LtS, Select]);
            }
            Bitwise::Abs => {
                self.push_as(code, left, Repr::Double);
                code.push(F64Abs);
            }
            Bitwise::UncheckedNegate => {
                if kind == Repr::Long { code.push(I64Const(0)); }
                self.push_as(code, left, kind);
                code.push(if kind == Repr::Long { I64Sub } else { F64Neg });
            }
            Bitwise::UncheckedSubtract if kind == Repr::Double => {
                self.boxed_double_subtraction(code, arguments, target);
                return;
            }
            Bitwise::UncheckedAdd | Bitwise::UncheckedSubtract | Bitwise::UncheckedSubtractPrimitive | Bitwise::UncheckedMultiply => {
                for argument in arguments { self.push_as(code, *argument, kind); }
                code.push(match (operation, kind) {
                    (Bitwise::UncheckedAdd, Repr::Long) => I64Add,
                    (Bitwise::UncheckedSubtract | Bitwise::UncheckedSubtractPrimitive, Repr::Long) => I64Sub,
                    (Bitwise::UncheckedMultiply, Repr::Long) => I64Mul,
                    (Bitwise::UncheckedAdd, _) => F64Add,
                    (Bitwise::UncheckedSubtractPrimitive, _) => F64Sub,
                    _ => F64Mul,
                });
            }
            Bitwise::Quot | Bitwise::Rem => {
                let right = arguments[1];
                self.push_as(code, right, kind);
                if kind == Repr::Long { code.push(I64Eqz); }
                else { code.extend([F64Const(0.0.into()), F64Eq]); }
                code.push(If(BlockType::Empty));
                self.slow(code, arguments, slow);
                code.push(End);
                if kind == Repr::Long {
                    if operation == Bitwise::Quot {
                        // Java wraps MIN / -1; Wasm div_s would trap.
                        code.extend([LocalGet(self.raw(right)), I64Const(-1), I64Eq,
                            If(BlockType::Result(ValType::I64)),
                            I64Const(0), LocalGet(self.raw(left)), I64Sub, Else,
                            LocalGet(self.raw(left)), LocalGet(self.raw(right)), I64DivS, End]);
                    } else {
                        code.extend([LocalGet(self.raw(left)), LocalGet(self.raw(right)), I64RemS]);
                    }
                } else {
                    self.push_as(code, left, Repr::Double);
                    self.push_as(code, right, Repr::Double);
                    code.extend([F64Div, LocalSet(target), LocalGet(target), F64Abs,
                        F64Const(f64::INFINITY.into()), F64Lt, I32Eqz, If(BlockType::Empty)]);
                    self.slow(code, arguments, slow);
                    code.extend([End, LocalGet(target), F64Abs,
                        F64Const(9_223_372_036_854_775_808.0f64.into()), F64Lt,
                        If(BlockType::Empty), LocalGet(target), I64TruncF64S,
                        F64ConvertI64S, LocalSet(target), End]);
                    if operation == Bitwise::Rem {
                        // The quotient's long round trip makes -0.0 positive;
                        // subtraction then retains the original dividend sign.
                        self.push_as(code, left, Repr::Double);
                        code.push(LocalGet(target));
                        self.push_as(code, right, Repr::Double);
                        code.extend([F64Mul, F64Sub]);
                    } else { code.push(LocalGet(target)); }
                }
            }
            _ => unreachable!("raw numeric core operation"),
        }
        code.push(LocalSet(target));
    }
}
