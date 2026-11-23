//! A host-installed entropy capability. The runtime itself remains import-free.
//! The callback is rooted in a typed global, outside the shared GC ABI prelude.
use super::*;
use Instruction::*;

pub(super) fn append(b: &mut Builder, globals: &mut GlobalSection) {
    let source = b.next_type;
    b.types.ty().function([], [ValType::I64]);
    b.next_type += 1;
    let reference = RefType {
        nullable: true,
        heap_type: HeapType::Concrete(source),
    };
    let slot = globals.len();
    globals.global(
        GlobalType {
            val_type: ValType::Ref(reference),
            mutable: true,
            shared: false,
        },
        &ConstExpr::ref_null(reference.heap_type),
    );
    b.function(
        "random-source-install",
        &[ValType::Ref(reference)],
        &[],
        &[LocalGet(0), GlobalSet(slot)],
    );
    let mut body = vec![GlobalGet(slot), RefIsNull, If(BlockType::Empty)];
    body.extend(numeric::error("Randomness capability is unavailable", 3));
    body.extend([
        End,
        GlobalGet(slot),
        RefAsNonNull,
        CallRef(source),
        // Use the high 53 bits of one uniformly distributed word. Every result
        // is an exactly representable binary64 in [0, 1), including zero.
        I64Const(11),
        I64ShrU,
        F64ConvertI64U,
        F64Const((1.0 / 9007199254740992.0).into()),
        F64Mul,
    ]);
    let sample = b.function("random-double-raw", &[], &[ValType::F64], &body);
    b.function(
        "random-double",
        &[],
        &[VALUE],
        &[Call(sample), StructNew(NUMBER)],
    );
}
