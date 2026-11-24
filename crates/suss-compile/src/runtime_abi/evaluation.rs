//! Native compiled-host capability plumbing. This does not implement eval or
//! interpret form data; a host compiler must install the execution callback.
//! The nullable callback root is appended outside the shared ABI type prelude.
use super::*;
use Instruction::*;

pub(super) fn append(b: &mut Builder, globals: &mut GlobalSection) {
    let source = b.next_type;
    b.types.ty().function([VALUE], [VALUE]);
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
        "evaluation-source-install",
        &[ValType::Ref(reference)],
        &[],
        &[LocalGet(0), GlobalSet(slot)],
    );
    let mut body = vec![GlobalGet(slot), RefIsNull, If(BlockType::Empty)];
    body.extend(numeric::error(
        "Compiled evaluation capability is unavailable",
        3,
    ));
    body.extend([
        End,
        LocalGet(0),
        GlobalGet(slot),
        RefAsNonNull,
        CallRef(source),
    ]);
    b.function("evaluate-form", &[VALUE], &[VALUE], &body);
}
