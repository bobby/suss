//! Shared native Number class diagnostics for constructors and intValue.
use super::*;
use Instruction::*;
fn error(message: &str) -> Vec<Instruction<'static>> {
    numeric::error(message, nominal::ERROR_GLOBAL)
}
fn class_cast(class: &str) -> Vec<Instruction<'static>> {
    let message = if class.starts_with("java.") {
        format!(
            "class {class} cannot be cast to class java.lang.Number ({class} and java.lang.Number are in module java.base of loader 'bootstrap')"
        )
    } else {
        format!(
            "class {class} cannot be cast to class java.lang.Number ({class} is in unnamed module of loader 'app'; java.lang.Number is in module java.base of loader 'bootstrap')"
        )
    };
    error(&message)
}
pub(super) fn nonnumber_errors(code: &mut Vec<Instruction<'static>>, nil_message: &str) {
    code.extend([LocalGet(0), RefIsNull, If(BlockType::Empty)]);
    code.extend(error(nil_message));
    code.push(End);
    code.extend([
        LocalGet(0),
        RefTestNonNull(HeapType::Concrete(STRING)),
        If(BlockType::Empty),
    ]);
    code.extend(class_cast("java.lang.String"));
    code.push(End);
    code.extend([
        LocalGet(0),
        GlobalGet(FALSE_GLOBAL),
        RefEq,
        LocalGet(0),
        GlobalGet(TRUE_GLOBAL),
        RefEq,
        I32Or,
        If(BlockType::Empty),
    ]);
    code.extend(class_cast("java.lang.Boolean"));
    code.push(End);
    code.extend([
        LocalGet(0),
        RefTestNonNull(HeapType::Concrete(OBJECT)),
        If(BlockType::Empty),
    ]);
    // Closed canonical collection descriptors identify their actual classes;
    // a lookalike user descriptor cannot acquire these primitive diagnostics.
    for (root, class) in [
        (KEYWORD_DESCRIPTOR_GLOBAL, "clojure.lang.Keyword"),
        (VECTOR_DESCRIPTOR_GLOBAL, "clojure.lang.PersistentVector"),
        (
            ARRAY_MAP_DESCRIPTOR_GLOBAL,
            "clojure.lang.PersistentArrayMap",
        ),
        (HASH_MAP_DESCRIPTOR_GLOBAL, "clojure.lang.PersistentHashMap"),
        (HASH_SET_DESCRIPTOR_GLOBAL, "clojure.lang.PersistentHashSet"),
    ] {
        code.extend([
            LocalGet(0),
            RefCastNonNull(HeapType::Concrete(OBJECT)),
            StructGet {
                struct_type_index: OBJECT,
                field_index: 0,
            },
            GlobalGet(root),
            RefEq,
            If(BlockType::Empty),
        ]);
        code.extend(class_cast(class));
        code.push(End);
    }
    code.push(End);
}
