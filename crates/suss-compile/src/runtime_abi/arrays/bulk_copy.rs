//! Guarded object-array copies for core's forward and backward loops (#479).
use super::*;

pub(super) fn functions(b: &mut Builder) {
    use Instruction::*;
    let args = ValType::Ref(RefType { nullable: true, heap_type: HeapType::Concrete(ARGS) });
    let i31 = HeapType::Abstract { shared: false, ty: AbstractHeapType::I31 };
    for backward in [false, true] {
        // Parameters: from, i, to, j, length. Locals: the two storages,
        // then the three nonnegative i31 offsets/count. No callback or
        // allocation can change their lengths between admission and copying.
        let mut code = vec![Block(BlockType::Result(VALUE))];
        let cast = |heap_type| BrOnCastFail {
            relative_depth: 0, from_ref_type: RefType::EQREF,
            to_ref_type: RefType { nullable: false, heap_type },
        };
        for (parameter, local) in [(1, 7), (3, 8), (4, 9)] {
            code.extend([LocalGet(parameter), cast(i31), I31GetS, LocalSet(local)]);
        }
        // The forward loop does not inspect its arrays for an empty copy.
        // The backward loop first computes i-1 and j-1; these i31 inputs
        // cannot overflow a long and those calculations have no effects.
        code.extend([LocalGet(9), I32Eqz, If(BlockType::Empty), LocalGet(2), Return, End,
            LocalGet(7), LocalGet(8), I32Or, LocalGet(9), I32Or, I32Const(0), I32LtS]);
        decline_if(&mut code);
        for (parameter, storage) in [(0, 5), (2, 6)] {
            code.extend([LocalGet(parameter), cast(HeapType::Concrete(PACKED)),
                StructGet { struct_type_index: PACKED, field_index: 0 },
                cast(HeapType::Concrete(ARGS)), LocalSet(storage)]);
        }
        // Two positive i31s sum to at most 2^31-2, so neither addition can
        // wrap. Declining an invalid full range preserves the old loops'
        // partial writes and exception order rather than throwing early.
        for (storage, offset) in [(5, 7), (6, 8)] {
            code.extend([LocalGet(offset), LocalGet(9), I32Add,
                LocalGet(storage), RefAsNonNull, ArrayLen, I32GtU]);
            decline_if(&mut code);
        }
        // array.copy is memmove. A sequential forward loop propagates when
        // its destination starts inside its source; the backward loop has
        // the opposite hazard. Compare storage identity, not wrapper identity.
        let (start, other) = if backward { (7, 8) } else { (8, 7) };
        code.extend([LocalGet(5), LocalGet(6), RefEq, If(BlockType::Empty),
            LocalGet(start), LocalGet(other), I32GtU,
            LocalGet(start), LocalGet(other), LocalGet(9), I32Add, I32LtU, I32And]);
        decline_if(&mut code);
        code.extend([End, LocalGet(6), RefAsNonNull, LocalGet(8),
            LocalGet(5), RefAsNonNull, LocalGet(7), LocalGet(9),
            ArrayCopy { array_type_index_dst: ARGS, array_type_index_src: ARGS },
            LocalGet(2), Return, End, Drop, sentinel_value(UNDEFINED)]);
        let name = if backward { "object-array-copy-backward" } else { "object-array-copy-forward" };
        b.function_with_locals(name, &[VALUE; 5], &[VALUE], &[(2, args), (3, ValType::I32)], &code);
    }
}

fn decline_if(code: &mut Vec<Instruction<'static>>) {
    use Instruction::*;
    code.extend([If(BlockType::Empty), sentinel_value(UNDEFINED), Return, End]);
}
