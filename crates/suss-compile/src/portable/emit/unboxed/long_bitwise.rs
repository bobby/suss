//! Raw counterparts of the strict 64-bit bitwise helpers.
use super::*;
pub(super) fn accepts(operation: Bitwise, count: usize) -> bool {
    matches!((operation, count), (Bitwise::Not, 1)
        | (Bitwise::And | Bitwise::Or | Bitwise::Xor | Bitwise::AndNot
            | Bitwise::Clear | Bitwise::Flip | Bitwise::Set | Bitwise::Test
            | Bitwise::Left | Bitwise::Right | Bitwise::Unsigned, 2))
}
impl Locals {
    pub(super) fn long_bitwise_raw(&self, code: &mut Vec<Instruction<'static>>,
        operation: Bitwise, arguments: &[ValueId], result: ValueId)
    {
        use Instruction::*;
        code.push(LocalGet(self.raw(arguments[0])));
        match operation {
            Bitwise::Not => code.extend([I64Const(-1), I64Xor]),
            Bitwise::Clear | Bitwise::Flip | Bitwise::Set => {
                code.extend([I64Const(1), LocalGet(self.raw(arguments[1])), I64Shl]);
                match operation {
                    Bitwise::Clear => code.extend([I64Const(-1), I64Xor, I64And]),
                    Bitwise::Flip => code.push(I64Xor),
                    _ => code.push(I64Or),
                }
            }
            _ => {
                code.push(LocalGet(self.raw(arguments[1])));
                match operation {
                    Bitwise::And => code.push(I64And), Bitwise::Or => code.push(I64Or),
                    Bitwise::Xor => code.push(I64Xor),
                    Bitwise::AndNot => code.extend([I64Const(-1), I64Xor, I64And]),
                    Bitwise::Left => code.push(I64Shl), Bitwise::Right => code.push(I64ShrS),
                    Bitwise::Unsigned => code.push(I64ShrU),
                    Bitwise::Test => code.extend([I64ShrU, I64Const(1), I64And, I32WrapI64]),
                    _ => unreachable!("raw long bit operation"),
                }
            }
        }
        code.push(LocalSet(self.raw(result)));
    }
}
