//! Checked/unchecked casts on raw values; slow paths preserve original errors.
use super::*;
pub(super) fn index(operation: Bitwise, arguments: &[ValueId], literals: &[Option<i64>]) -> Option<usize> {
    if arguments.len() == 1 {
        match operation {
            Bitwise::Int => return Some(12), Bitwise::IntPrimitive => return Some(1),
            Bitwise::IntFloat => return Some(15), _ => {}
        }
    }
    if operation != Bitwise::Cast || arguments.len() != 2 { return None; }
    literals[arguments[1].0].filter(|index| (0..=15).contains(index)).map(|index| index as usize)
}
pub(super) fn result(index: usize) -> Repr {
    if (8..=11).contains(&index) { Repr::Double } else { Repr::Long }
}
impl Locals {
    pub(super) fn numeric_cast_raw(&self, code: &mut Vec<Instruction<'static>>, index: usize,
        arguments: &[ValueId], result: ValueId, slow: u32)
    {
        use Instruction::*;
        let value = arguments[0];
        let from = self.of(value).definite();
        let raw = self.raw(value);
        match index {
            0..=3 => {
                let bits = [64, 32, 16, 8][index];
                if from == Repr::Long {
                    if bits < 64 {
                        let shift = I64Const(64 - bits);
                        code.extend([LocalGet(raw), shift.clone(), I64Shl, shift, I64ShrS,
                            LocalGet(raw), I64Ne, If(BlockType::Empty)]);
                        self.slow(code, arguments, slow);
                        code.push(End);
                    }
                    code.push(LocalGet(raw));
                } else {
                    let bound = 2_f64.powi((bits - 1) as i32);
                    let high = if bits == 64 { bound } else { bound - 1.0 };
                    if bits >= 32 {
                        code.extend([LocalGet(raw), F64Const((-bound).into()), F64Lt,
                            LocalGet(raw), F64Const(high.into()), F64Gt, I32Or]);
                    } else {
                        code.extend([LocalGet(raw), F64Const((-bound).into()), F64Ge,
                            LocalGet(raw), F64Const(high.into()), F64Le, I32And, I32Eqz]);
                    }
                    code.push(If(BlockType::Empty));
                    self.slow(code, arguments, slow);
                    code.extend([End, LocalGet(raw), I64TruncSatF64S]);
                }
            }
            12..=14 => {
                let bits = [32, 16, 8][index - 12];
                let converted = self.raw(result);
                code.push(LocalGet(raw));
                if from == Repr::Double { code.push(I64TruncSatF64S); }
                code.extend([LocalSet(converted), LocalGet(converted), I64Const(64 - bits),
                    I64Shl, I64Const(64 - bits), I64ShrS, LocalGet(converted), I64Ne,
                    If(BlockType::Empty)]);
                self.slow(code, arguments, slow);
                code.extend([End, LocalGet(converted)]);
            }
            15 => {
                self.push_as(code, value, Repr::Double);
                code.extend([F64Const((-2147483648.0).into()), F64Lt]);
                self.push_as(code, value, Repr::Double);
                code.extend([F64Const(2147483648.0.into()), F64Gt, I32Or, If(BlockType::Empty)]);
                self.slow(code, arguments, slow);
                code.push(End);
                self.push_as(code, value, Repr::Double);
                code.extend([I32TruncSatF64S, I64ExtendI32S]);
            }
            4..=7 => {
                let bits = [64, 32, 16, 8][index - 4];
                code.push(LocalGet(raw));
                if from == Repr::Double {
                    if bits == 64 { code.push(I64TruncSatF64S); }
                    else { code.extend([I32TruncSatF64S, I64ExtendI32S]); }
                }
                if bits < 64 {
                    code.extend([I64Const(64 - bits), I64Shl, I64Const(64 - bits), I64ShrS]);
                }
            }
            8 | 11 => {
                if index == 11 && from == Repr::Long {
                    code.extend([LocalGet(raw), F32ConvertI64S, F64PromoteF32]);
                } else {
                    self.push_as(code, value, Repr::Double);
                    code.extend([F64Abs, F64Const(f64::from(f32::MAX).into()), F64Gt, If(BlockType::Empty)]);
                    self.slow(code, arguments, slow);
                    code.push(End);
                    self.push_as(code, value, Repr::Double);
                    code.extend([F32DemoteF64, F64PromoteF32]);
                }
            }
            9 => {
                code.push(LocalGet(raw));
                code.push(if from == Repr::Long { F32ConvertI64S } else { F32DemoteF64 });
                code.push(F64PromoteF32);
            }
            10 => self.push_as(code, value, Repr::Double),
            _ => unreachable!("numeric cast index"),
        }
        code.push(LocalSet(self.raw(result)));
    }
}
