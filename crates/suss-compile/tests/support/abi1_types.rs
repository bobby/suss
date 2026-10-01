//! Frozen ABI1 recursive group from reviewed8e0f3b5; development compatibility fixture.
//! Keep this historical shape when the production group changes.
use wasm_encoder::*;
const NUMBER: u32 = 0;
const STRING: u32 = 1;
const ARGS: u32 = 2;
const INVOKE: u32 = 3;
const DESCRIPTOR: u32 = 6;
const VALUE: ValType = ValType::Ref(RefType::EQREF);
fn reference(index: u32) -> ValType {
    ValType::Ref(RefType { nullable: false, heap_type: HeapType::Concrete(index) })
}
fn field(ty: ValType, mutable: bool) -> FieldType {
    FieldType { element_type: StorageType::Val(ty), mutable }
}
fn structure(fields: Vec<FieldType>) -> CompositeInnerType {
    CompositeInnerType::Struct(StructType { fields: fields.into_boxed_slice() })
}
pub fn prelude() -> TypeSection {
    let mut types = TypeSection::new();
    let group = vec![
        structure(vec![field(ValType::F64, false)]),
        CompositeInnerType::Array(ArrayType(FieldType {
            element_type: StorageType::I16,
            mutable: true,
        })),
        CompositeInnerType::Array(ArrayType(field(VALUE, true))),
        CompositeInnerType::Func(FuncType::new([VALUE, reference(ARGS)], [VALUE])),
        // Closure: environment, invoke, minimum arity, maximum arity (-1 variadic).
        structure(vec![
            field(VALUE, false),
            field(reference(INVOKE), false),
            field(ValType::I32, false),
            field(ValType::I32, false),
        ]),
        // Binding cell: value and independent bound flag (nil can be a binding).
        structure(vec![field(VALUE, true), field(ValType::I32, true)]),
        // Descriptor: nominal identity, field schema, protocol table, metadata.
        structure(vec![
            field(ValType::I64, false),
            field(VALUE, false),
            field(VALUE, true),
            field(VALUE, true),
        ]),
        // User object: descriptor, fields, per-object metadata.
        structure(vec![
            field(reference(DESCRIPTOR), false),
            field(reference(ARGS), false),
            field(VALUE, false),
        ]),
        // Exception: descriptor, message, data, cause.
        structure(vec![
            field(reference(DESCRIPTOR), false),
            field(reference(STRING), false),
            field(VALUE, false),
            field(VALUE, false),
        ]),
        // Dynamic frame: parent and binding entries; scheduler roots it as Value.
        structure(vec![field(VALUE, false), field(reference(ARGS), false)]),
    ];
    types.ty().rec(group.into_iter().map(|inner| SubType {
        is_final: true,
        supertype_idx: None,
        composite_type: CompositeType {
            inner,
            shared: false,
            descriptor: None,
            describes: None,
        },
    }));
    types
}

