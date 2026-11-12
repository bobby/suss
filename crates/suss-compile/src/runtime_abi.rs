//! Shared GC ABI v2. The legacy backend has not migrated to this ABI.
//! All ordinary numbers are boxed binary64; strings store UTF-16 units.
//! Construction indices below are implementation details, not artifact ABI IDs.
use std::borrow::Cow;
use wasm_encoder::*;
mod arithmetic;
mod array_join;
mod array_methods;
mod array_own_properties;
mod array_pop;
mod array_shift;
mod array_properties;
mod array_push;
mod arrays;
mod r#async;
mod async_library;
mod bitwise;
mod callable_properties;
mod closure_calls;
mod closure_properties;
mod coercions;
mod comparisons;
mod dynamic;
mod error_constructors;
mod error_properties;
mod exception_info;
mod exceptions;
mod identity_hash;
mod longs;
mod named_properties;
mod native_object_factory;
mod native_object_methods;
mod native_object_properties;
mod native_objects;
mod native_protocols;
mod nominal;
mod numeric;
mod packed_arrays;
mod numeric_hash;
mod numeric_operands;
mod object_descriptors;
mod object_methods;
mod owned_prototypes;
mod predicates;
mod primitive_constructors;
mod range_errors;
mod sparse_arrays;
mod streams;
mod string_methods;
mod string_char_at;
mod type_values;

pub const VERSION: u32 = 5;
// The language sentinels. Every i31 is a long (ADR-0003); nil is a null
// eqref, and false, true and undefined are SENTINEL structs held in runtime
// globals that fragments import. Code names a sentinel with these and builds
// it with `sentinel_value`; the numbers are only the SENTINEL struct's tag.
pub const NIL: i32 = 0;
pub const FALSE: i32 = 2;
pub const TRUE: i32 = 4;
// Internal constructor/ordinary-type-call undefined, distinct from source nil.
pub const UNDEFINED: i32 = 6;
// Right after the numeric stack global, before any initializer that uses them.
pub(crate) const FALSE_GLOBAL: u32 = numeric::ERROR_GLOBALS + 1;
pub(crate) const TRUE_GLOBAL: u32 = FALSE_GLOBAL + 1;
pub(crate) const UNDEFINED_GLOBAL: u32 = TRUE_GLOBAL + 1;
/// Runtime export names of the sentinel globals, in tag order.
pub const SENTINEL_EXPORTS: [(&str, i32); 3] =
    [("sentinel-false", FALSE), ("sentinel-true", TRUE), ("sentinel-undefined", UNDEFINED)];

/// A sentinel inside the runtime module, where its globals are local.
pub(crate) fn sentinel_value(value: i32) -> Instruction<'static> {
    sentinel_in(value, FALSE_GLOBAL)
}
/// A sentinel in a module whose false, true and undefined globals start at
/// `first` in tag order, such as a fragment that imports them.
pub fn sentinel_in(value: i32, first: u32) -> Instruction<'static> {
    match value {
        NIL => Instruction::RefNull(HeapType::Abstract {
            shared: false,
            ty: AbstractHeapType::Eq,
        }),
        FALSE => Instruction::GlobalGet(first),
        TRUE => Instruction::GlobalGet(first + 1),
        UNDEFINED => Instruction::GlobalGet(first + 2),
        other => panic!("not a sentinel: {other}"),
    }
}
/// `[cond] -> [boolean]`: true for a nonzero i32, false for zero.
pub fn boolean_in(first: u32) -> [Instruction<'static>; 5] {
    [
        Instruction::If(BlockType::Result(VALUE)),
        Instruction::GlobalGet(first + 1),
        Instruction::Else,
        Instruction::GlobalGet(first),
        Instruction::End,
    ]
}
/// `[] -> [i32]`: whether a local holds a number, a long or a double.
pub fn number_test(local: u32) -> [Instruction<'static>; 8] {
    longs::number_test(local)
}
/// `[] -> [f64]`: the number in `local` as a double; traps unless it is one.
pub fn number_to_f64(local: u32) -> Vec<Instruction<'static>> {
    use Instruction::*;
    vec![
        LocalGet(local),
        RefTestNonNull(HeapType::Concrete(NUMBER)),
        If(BlockType::Result(ValType::F64)),
        LocalGet(local),
        RefCastNonNull(HeapType::Concrete(NUMBER)),
        StructGet {
            struct_type_index: NUMBER,
            field_index: 0,
        },
        Else,
        LocalGet(local),
        RefTestNonNull(HeapType::I31),
        If(BlockType::Result(ValType::F64)),
        LocalGet(local),
        RefCastNonNull(HeapType::I31),
        I31GetS,
        F64ConvertI32S,
        Else,
        LocalGet(local),
        RefCastNonNull(HeapType::Concrete(LONG)),
        StructGet {
            struct_type_index: LONG,
            field_index: 0,
        },
        F64ConvertI64S,
        End,
        End,
    ]
}
/// `[] -> [i32]`: whether a local holds a long.
pub fn long_test(local: u32) -> [Instruction<'static>; 5] {
    use Instruction::*;
    [
        LocalGet(local),
        RefTestNonNull(HeapType::I31),
        LocalGet(local),
        RefTestNonNull(HeapType::Concrete(LONG)),
        I32Or,
    ]
}
/// `[] -> [i64]`: the long in `local`; traps unless it is one.
pub fn long_to_i64(local: u32) -> Vec<Instruction<'static>> {
    use Instruction::*;
    vec![
        LocalGet(local),
        RefTestNonNull(HeapType::I31),
        If(BlockType::Result(ValType::I64)),
        LocalGet(local),
        RefCastNonNull(HeapType::I31),
        I31GetS,
        I64ExtendI32S,
        Else,
        LocalGet(local),
        RefCastNonNull(HeapType::Concrete(LONG)),
        StructGet {
            struct_type_index: LONG,
            field_index: 0,
        },
        End,
    ]
}
/// `[i64] -> [eqref]`: a long, as an i31 when it fits and boxed otherwise,
/// using `scratch`, an `i64` local.
pub fn long_box_in(scratch: u32) -> Vec<Instruction<'static>> {
    use Instruction::*;
    let mut code = vec![LocalTee(scratch)];
    code.extend(longs::fits_i31());
    code.extend([
        If(BlockType::Result(VALUE)),
        LocalGet(scratch),
        I32WrapI64,
        RefI31,
        Else,
        LocalGet(scratch),
        StructNew(LONG),
        End,
    ]);
    code
}
pub(crate) fn boolean() -> [Instruction<'static>; 5] {
    boolean_in(FALSE_GLOBAL)
}
const PORTABLE_FORMAT_VERSION: u32 = 3;
const MANIFEST: &str = "suss.runtime-abi";
pub(crate) const NUMBER: u32 = 0;
pub(crate) const STRING: u32 = 1;
pub(crate) const ARGS: u32 = 2;
pub(crate) const INVOKE: u32 = 3;
pub const CLOSURE: u32 = 4;
pub(crate) const BINDING: u32 = 5;
/// Bits of a binding cell's flag field. A cell is dynamic once a binding frame
/// has held it, and stays so; only a dynamic cell's reads search the frames, so
/// a read of a bound, never-bound-dynamically cell is its root (#354).
pub(crate) const CELL_BOUND: i32 = 1;
pub(crate) const CELL_DYNAMIC: i32 = 2;
const DESCRIPTOR: u32 = 6;
pub const SENTINEL: u32 = 10;
pub const LONG: u32 = 11;
/// Packed primitive array storage, one type per element type, in
/// `packed_arrays::KINDS` order: byte, short, int, float, long, double (#336).
pub const PACKED_STORAGE: u32 = 12;
/// A packed primitive array: its storage and an identity UID.
pub const PACKED: u32 = 18;
/// ADR-0004's calling convention (ABI 4). `CLOSURE` is the non-final
/// `$closure {code, properties, uid}`; `CODE` is a function's shared code
/// struct `{name, e0 … e8, spread}`; `CALL + k` is `$callk`, the entry for k
/// arguments, `(ref $closure) eqref × k -> eqref`; `SPREAD` is the spread entry,
/// `(ref $closure) (ref $args) eqref -> eqref`, taking realized arguments and
/// the unrealized rest, a seq or nil. `UNIVERSAL` is the closure subtype the
/// runtime and host make: `{code, properties, uid, environment, invoke,
/// minimum arity, maximum arity (-1 variadic)}`, called through one shared
/// code struct whose entries pack their arguments for `invoke`.
pub const MAX_FIXED: u32 = 8;
pub const CODE: u32 = 19;
pub const CALL: u32 = 20;
pub const SPREAD: u32 = CALL + MAX_FIXED + 1;
pub const UNIVERSAL: u32 = SPREAD + 1;
/// ABI 5 (#378): a cached named-read site's state, one struct per site held in
/// an immutable global, so the site's code is a call to `named-site-get`:
/// `{descriptor, slot (-1 for a method), method, epoch, second descriptor,
/// second epoch, second method, function owner, owner epoch, owner value}`.
pub const NAMED_SITE: u32 = UNIVERSAL + 1;
pub const TYPE_COUNT: u32 = NAMED_SITE + 1;
/// `NAMED_SITE` fields.
pub(crate) const SITE_DESCRIPTOR: u32 = 0;
pub(crate) const SITE_SLOT: u32 = 1;
pub(crate) const SITE_METHOD: u32 = 2;
pub(crate) const SITE_EPOCH: u32 = 3;
pub(crate) const SITE_DESCRIPTOR_2: u32 = 4;
pub(crate) const SITE_EPOCH_2: u32 = 5;
pub(crate) const SITE_METHOD_2: u32 = 6;
pub(crate) const SITE_OWNER: u32 = 7;
pub(crate) const SITE_OWNER_EPOCH: u32 = 8;
pub(crate) const SITE_OWNER_VALUE: u32 = 9;
/// A new site's fields: no descriptor or owner (none is null) and no epoch.
pub fn named_site_initializer() -> Vec<Instruction<'static>> {
    use Instruction::*;
    let null = || RefNull(HeapType::Abstract { shared: false, ty: AbstractHeapType::Eq });
    vec![
        null(), I32Const(-1), null(), I32Const(-1),
        null(), I32Const(-1), null(),
        null(), I32Const(-1), null(),
        StructNew(NAMED_SITE),
    ]
}
const VALUE: ValType = ValType::Ref(RefType::EQREF);

/// `named-site-get(site, object, name)` (#378): a core named read through its
/// site's cache. An object with the site's descriptor reads the cached field,
/// or the cached method while the protocol epoch is unchanged; the second
/// method entry serves a site that alternates between two types; a function
/// owner reads its cached named property. Otherwise the field slot, the
/// descriptor's own method or the function's own property is looked up and
/// cached, and anything else takes `named-property-get`, uncached.
fn named_site_get(b: &mut Builder) {
    use Instruction::*;
    // Params: 0 site, 1 object, 2 name. Locals: 3 descriptor, 4 slot, 5 value.
    let site = |field| [LocalGet(0), StructGet { struct_type_index: NAMED_SITE, field_index: field }];
    let set = |field: u32, value: Vec<Instruction<'static>>| {
        let mut code = vec![LocalGet(0)];
        code.extend(value);
        code.push(StructSet { struct_type_index: NAMED_SITE, field_index: field });
        code
    };
    let epoch = || vec![GlobalGet(PROTOCOL_EPOCH_GLOBAL)];
    let property_get = b.names["named-property-get"];
    let full = || [LocalGet(1), LocalGet(2), Call(property_get), Return];
    let field_read = |slot: u32| [
        LocalGet(1),
        RefCastNonNull(HeapType::Concrete(7)),
        StructGet { struct_type_index: 7, field_index: 1 },
        RefCastNonNull(HeapType::Concrete(ARGS)),
        LocalGet(slot),
        ArrayGet(ARGS),
        Return,
    ];
    // The descriptor's own method, cached in the first entry (its method
    // moves to the second), or the full lookup when it has none.
    let mut method = vec![
        LocalGet(3), LocalGet(2), Call(b.names["descriptor-method-get"]), LocalTee(5),
        RefIsNull, LocalGet(5), sentinel_value(NIL), RefEq, I32Or, If(BlockType::Empty),
    ];
    method.extend(full());
    method.push(End);
    method.extend(site(SITE_SLOT));
    method.extend([I32Const(0), I32LtS, If(BlockType::Empty)]);
    method.extend(set(SITE_DESCRIPTOR_2, site(SITE_DESCRIPTOR).to_vec()));
    method.extend(set(SITE_EPOCH_2, site(SITE_EPOCH).to_vec()));
    method.extend(set(SITE_METHOD_2, site(SITE_METHOD).to_vec()));
    method.push(End);
    method.extend(set(SITE_DESCRIPTOR, vec![LocalGet(3)]));
    method.extend(set(SITE_SLOT, vec![I32Const(-1)]));
    method.extend(set(SITE_METHOD, vec![LocalGet(5)]));
    method.extend(set(SITE_EPOCH, epoch()));
    method.extend([LocalGet(5), Return]);

    let mut code = vec![LocalGet(1), RefTestNonNull(HeapType::Concrete(7)), If(BlockType::Empty)];
    code.extend([LocalGet(1), RefCastNonNull(HeapType::Concrete(7)), StructGet { struct_type_index: 7, field_index: 0 }, LocalTee(3)]);
    code.extend(site(SITE_DESCRIPTOR));
    code.extend([RefEq, If(BlockType::Empty)]);
    code.extend(site(SITE_SLOT));
    code.extend([LocalTee(4), I32Const(0), I32GeS, If(BlockType::Empty)]);
    code.extend(field_read(4));
    code.push(End);
    code.extend(site(SITE_EPOCH));
    code.extend(epoch());
    code.extend([I32Eq, If(BlockType::Empty)]);
    code.extend(site(SITE_METHOD));
    code.extend([Return, End]);
    code.extend(method.clone());
    code.push(End);
    code.push(LocalGet(3));
    code.extend(site(SITE_DESCRIPTOR_2));
    code.push(RefEq);
    code.extend(site(SITE_EPOCH_2));
    code.extend(epoch());
    code.extend([I32Eq, I32And, If(BlockType::Empty)]);
    code.extend(site(SITE_METHOD_2));
    code.extend([Return, End]);
    code.extend([LocalGet(1), LocalGet(2), Call(b.names["named-field-slot"]), LocalTee(4), I32Const(0), I32GeS, If(BlockType::Empty)]);
    code.extend(set(SITE_DESCRIPTOR, vec![LocalGet(3)]));
    code.extend(set(SITE_SLOT, vec![LocalGet(4)]));
    code.extend(field_read(4));
    code.push(End);
    code.extend(method);
    code.push(End);
    // A function's own named property, such as a constructor static.
    code.extend([LocalGet(1), RefTestNonNull(HeapType::Concrete(CLOSURE)), If(BlockType::Empty)]);
    code.push(LocalGet(1));
    code.extend(site(SITE_OWNER));
    code.push(RefEq);
    code.extend(site(SITE_OWNER_EPOCH));
    code.extend(epoch());
    code.extend([I32Eq, I32And, If(BlockType::Empty)]);
    code.extend(site(SITE_OWNER_VALUE));
    code.extend([Return, End]);
    code.extend([LocalGet(1), LocalGet(2), Call(b.names["closure-named-get"]), LocalTee(5), RefIsNull, If(BlockType::Empty)]);
    code.extend(full());
    code.push(End);
    code.extend(set(SITE_OWNER, vec![LocalGet(1)]));
    code.extend(set(SITE_OWNER_EPOCH, epoch()));
    code.extend(set(SITE_OWNER_VALUE, vec![LocalGet(5)]));
    code.extend([LocalGet(5), Return, End]);
    code.extend(full());
    b.function_with_locals("named-site-get", &[reference(NAMED_SITE), VALUE, VALUE], &[VALUE], &[(1, VALUE), (1, ValType::I32), (1, VALUE)], &code);
}

/// `instance-site-test(site, constructor, value)` (#378): `instance?` through
/// its site's cache, a `NAMED_SITE` struct whose descriptor, method and
/// second-descriptor fields hold the last constructor, its descriptor and its
/// prototype, valid at the protocol epoch. The constructor is checked first;
/// an i31 is never an instance; an object of the descriptor other than its
/// prototype is one; anything else takes `object-instance`.
fn instance_site_test(b: &mut Builder) {
    use Instruction::*;
    // Params: 0 site, 1 constructor, 2 value.
    let site = |field| [LocalGet(0), StructGet { struct_type_index: NAMED_SITE, field_index: field }];
    let set = |field: u32, value: Vec<Instruction<'static>>| {
        let mut code = vec![LocalGet(0)];
        code.extend(value);
        code.push(StructSet { struct_type_index: NAMED_SITE, field_index: field });
        code
    };
    let (constructor, descriptor, prototype) = (SITE_DESCRIPTOR, SITE_METHOD, SITE_DESCRIPTOR_2);
    let mut code = vec![LocalGet(1)];
    code.extend(site(constructor));
    code.push(RefEq);
    code.extend(site(SITE_EPOCH));
    code.extend([GlobalGet(PROTOCOL_EPOCH_GLOBAL), I32Eq, I32And, I32Eqz, If(BlockType::Empty)]);
    code.extend(set(descriptor, vec![LocalGet(1), Call(b.names["constructor-descriptor"])]));
    code.extend(set(prototype, [site(descriptor).to_vec(), vec![Call(b.names["owned-descriptor-prototype"])]].concat()));
    code.extend(set(constructor, vec![LocalGet(1)]));
    code.extend(set(SITE_EPOCH, vec![GlobalGet(PROTOCOL_EPOCH_GLOBAL)]));
    code.push(End);
    code.extend([LocalGet(2), RefTestNonNull(HeapType::I31), If(BlockType::Empty), I32Const(0), Return, End]);
    code.extend([LocalGet(2), RefTestNonNull(HeapType::Concrete(7)), If(BlockType::Empty)]);
    code.extend([LocalGet(2), RefCastNonNull(HeapType::Concrete(7)), StructGet { struct_type_index: 7, field_index: 0 }]);
    code.extend(site(descriptor));
    code.extend([RefEq, LocalGet(2)]);
    code.extend(site(prototype));
    code.extend([RefEq, I32Eqz, I32And, If(BlockType::Empty), I32Const(1), Return, End, End]);
    code.extend(site(descriptor));
    code.extend([LocalGet(2), Call(b.names["object-instance"])]);
    b.function("instance-site-test", &[reference(NAMED_SITE), VALUE, VALUE], &[ValType::I32], &code);
}

fn reference(index: u32) -> ValType {
    ValType::Ref(RefType {
        nullable: false,
        heap_type: HeapType::Concrete(index),
    })
}
/// Exact type of the imported false, true and undefined globals.
pub fn sentinel_type() -> ValType {
    reference(SENTINEL)
}
/// A function's code struct reference, as a definition's global holds it.
pub(crate) fn code_type() -> ValType {
    reference(CODE)
}
/// The universal function type's reference, as `closure-new` takes it.
pub(crate) fn invoke_type() -> ValType {
    reference(INVOKE)
}
/// Exact shared binding-cell reference type for fragment imports.
pub(crate) fn binding_cell_type() -> ValType {
    reference(BINDING)
}
fn field(ty: ValType, mutable: bool) -> FieldType {
    FieldType {
        element_type: StorageType::Val(ty),
        mutable,
    }
}
fn structure(fields: Vec<FieldType>) -> CompositeInnerType {
    CompositeInnerType::Struct(StructType {
        fields: fields.into_boxed_slice(),
    })
}

/// The identical recursive group must begin at type index zero in every fragment.
/// Append fragment function types afterward; do not embed prototype GC layouts.
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
        // Closure (ADR-0004): its function's code struct, native protocol
        // properties or null, and the lazily assigned, owner-held identity UID.
        structure(closure_fields()),
        // Binding cell: value and flags, CELL_BOUND independent of the value
        // (nil can be a binding) and CELL_DYNAMIC.
        structure(vec![field(VALUE, true), field(ValType::I32, true)]),
        // Descriptor: nominal identity, field schema, protocol table, metadata.
        structure(vec![
            field(ValType::I64, false),
            field(VALUE, false),
            field(VALUE, true),
            field(VALUE, true),
            field(VALUE, true), // Identity UID, separate from metadata.
            // Munged names of `^:mutable` fields (ARGS of strings), or nil.
            field(VALUE, false),
        ]),
        // User object: descriptor, fields, per-object metadata.
        structure(vec![
            field(reference(DESCRIPTOR), false),
            field(reference(ARGS), false),
            field(VALUE, false),
            field(VALUE, true), // Identity UID.
        ]),
        // Exception: descriptor, message, data, cause.
        structure(vec![
            field(reference(DESCRIPTOR), false),
            field(reference(STRING), false),
            field(VALUE, false),
            field(VALUE, false),
            field(VALUE, true), // Identity UID; message/data/cause offsets unchanged.
        ]),
        // Dynamic frame: parent and binding entries; scheduler roots it as Value.
        structure(vec![field(VALUE, false), field(reference(ARGS), false)]),
        // ABI v3. Sentinel: false, true or undefined, by the tag it holds.
        structure(vec![field(ValType::I32, false)]),
        // Long outside i31's range; one in range is always an i31.
        structure(vec![field(ValType::I64, false)]),
    ];
    // Packed primitive array storage (#336), then the array value itself:
    // storage and a lazily assigned identity UID, as `hash` needs one.
    let mut group = group;
    for element in [
        StorageType::I8,
        StorageType::I16,
        StorageType::Val(ValType::I32),
        StorageType::Val(ValType::F32),
        StorageType::Val(ValType::I64),
        StorageType::Val(ValType::F64),
    ] {
        group.push(CompositeInnerType::Array(ArrayType(FieldType {
            element_type: element,
            mutable: true,
        })));
    }
    group.push(structure(vec![field(VALUE, false), field(VALUE, true)]));
    // ABI 4 (ADR-0004): the code struct, the entry types and the runtime's
    // universal closure, appended so no earlier index moves.
    let mut code = vec![field(VALUE, false)];
    code.extend((0..=MAX_FIXED).map(|k| field(reference(CALL + k), false)));
    code.push(field(reference(SPREAD), false));
    // The arity range, for JavaScript-convention method calls only.
    code.extend([field(ValType::I32, false), field(ValType::I32, false)]);
    group.push(structure(code));
    for k in 0..=MAX_FIXED {
        let mut params = vec![reference(CLOSURE)];
        params.extend((0..k).map(|_| VALUE));
        group.push(CompositeInnerType::Func(FuncType::new(params, [VALUE])));
    }
    group.push(CompositeInnerType::Func(FuncType::new(
        [reference(CLOSURE), reference(ARGS), VALUE],
        [VALUE],
    )));
    let mut universal = closure_fields();
    universal.extend([
        field(VALUE, false),
        field(reference(INVOKE), false),
        field(ValType::I32, false),
        field(ValType::I32, false),
    ]);
    group.push(structure(universal));
    let mutable = |ty| field(ty, true);
    group.push(structure(vec![
        mutable(VALUE), mutable(ValType::I32), mutable(VALUE), mutable(ValType::I32),
        mutable(VALUE), mutable(ValType::I32), mutable(VALUE),
        mutable(VALUE), mutable(ValType::I32), mutable(VALUE),
    ]));
    debug_assert_eq!(group.len() as u32, TYPE_COUNT);
    types.ty().rec(group.into_iter().enumerate().map(|(index, inner)| {
        let index = index as u32;
        SubType {
            // Only `$closure` has subtypes: the universal closure here, and
            // each compiled function's capture struct in its fragment.
            is_final: index != CLOSURE,
            supertype_idx: (index == UNIVERSAL).then_some(CLOSURE),
            composite_type: CompositeType {
                inner,
                shared: false,
                descriptor: None,
                describes: None,
            },
        }
    }));
    types
}

/// `$closure`'s fields, which every closure subtype begins with.
pub(crate) fn closure_fields() -> Vec<FieldType> {
    vec![
        field(reference(CODE), false),
        field(VALUE, true),
        field(VALUE, true),
    ]
}
/// Fields of `$closure` and of the universal closure.
pub(crate) const CLOSURE_CODE: u32 = 0;
pub(crate) const CLOSURE_PROPERTIES: u32 = 1;
pub(crate) const CLOSURE_UID: u32 = 2;
/// Fields every closure begins with; a capture struct's captures follow.
pub(crate) const CLOSURE_FIELDS: u32 = 3;
pub(crate) const UNIVERSAL_ENVIRONMENT: u32 = 3;
pub(crate) const UNIVERSAL_INVOKE: u32 = 4;
pub(crate) const UNIVERSAL_MIN: u32 = 5;
pub(crate) const UNIVERSAL_MAX: u32 = 6;
/// Code struct fields: the name, then entry k at `CODE_ENTRY + k`, then spread.
pub(crate) const CODE_NAME: u32 = 0;
pub(crate) const CODE_ENTRY: u32 = 1;
pub(crate) const CODE_SPREAD: u32 = CODE_ENTRY + MAX_FIXED + 1;
/// A definition's minimum and maximum arity (-1 when variadic). Clojure calls
/// never read them; JavaScript-convention method calls (`.method` interop)
/// fill missing operands with undefined and drop surplus ones by them.
pub(crate) const CODE_MIN: u32 = CODE_SPREAD + 1;
pub(crate) const CODE_MAX: u32 = CODE_SPREAD + 2;
/// Runtime exports by arity (ADR-0004): `call-k` takes any callee and k
/// arguments, the out-of-line path of a call site; `arity-error-k` is the
/// entry of type `$callk` for an arity a function lacks.
pub const CALL_EXPORTS: [&str; MAX_FIXED as usize + 1] =
    ["call-0", "call-1", "call-2", "call-3", "call-4", "call-5", "call-6", "call-7", "call-8"];
pub const ARITY_ERROR_EXPORTS: [&str; MAX_FIXED as usize + 1] = [
    "arity-error-0", "arity-error-1", "arity-error-2", "arity-error-3", "arity-error-4",
    "arity-error-5", "arity-error-6", "arity-error-7", "arity-error-8",
];
/// The runtime's universal code struct, shared by every universal closure.
pub(crate) const UNIVERSAL_CODE_GLOBAL: u32 = object_methods::CURRENT_THIS + 1;
/// IFn's `-invoke` keys for the out-of-line call path: an `ARGS` indexed by
/// signature arity (receiver included: 1 to 22, the last with `rest`), or
/// null before IFn.
pub(crate) const CALLABLE_KEYS_GLOBAL: u32 = UNIVERSAL_CODE_GLOBAL + 1;
const CALLABLE_KEY_COUNT: i32 = 23;
/// Protocol dispatchers' shared code struct (#355).
pub(crate) const DISPATCH_CODE_GLOBAL: u32 = CALLABLE_KEYS_GLOBAL + 1;
/// The interned keywords (#361): `KEYWORD_BUCKETS` chains of `[keyword,
/// next]`, by a hash of the full name, or null before the first.
pub(crate) const KEYWORDS_GLOBAL: u32 = DISPATCH_CODE_GLOBAL + 1;
/// The interned keywords' descriptor, from the first keyword interned, or null.
pub(crate) const KEYWORD_DESCRIPTOR_GLOBAL: u32 = KEYWORDS_GLOBAL + 1;
/// Bumped by every protocol method table write; a call site's method cache
/// holds only while it is unchanged (#355). Exported as `protocol-epoch`.
pub(crate) const PROTOCOL_EPOCH_GLOBAL: u32 = KEYWORD_DESCRIPTOR_GLOBAL + 1;
const KEYWORD_BUCKETS: i32 = 4096;

/// `keyword-intern(keyword)`: the first keyword interned with the same full
/// name, interning this one if it is the first (#361). A keyword is a user
/// object whose fields are `[ns, name, fqn, hash]`; anything else is returned
/// unchanged. Clojure interns keywords weakly; Suss holds them.
fn keyword_intern(b: &mut Builder) -> u32 {
    use Instruction::*;
    let string = HeapType::Concrete(STRING);
    let fields = |object: u32| {
        vec![
            LocalGet(object),
            RefCastNonNull(HeapType::Concrete(7)),
            StructGet { struct_type_index: 7, field_index: 1 },
            RefCastNonNull(HeapType::Concrete(ARGS)),
        ]
    };
    // Params: 0 keyword. Locals: 1 fqn, 2 hash, 3 index, 4 bucket table,
    // 5 node, 6 other fqn, 7 equal.
    let mut body = vec![
        LocalGet(0),
        RefTestNonNull(HeapType::Concrete(7)),
        I32Eqz,
        If(BlockType::Empty),
        LocalGet(0),
        Return,
        End,
    ];
    body.extend(fields(0));
    body.extend([ArrayLen, I32Const(4), I32Ne, If(BlockType::Empty), LocalGet(0), Return, End]);
    // Only a well-formed keyword is interned: a nil or string namespace and a
    // string name and full name. Anything else is returned as constructed.
    body.extend(fields(0));
    body.extend([I32Const(0), ArrayGet(ARGS), LocalTee(1), RefIsNull, LocalGet(1), RefTestNonNull(string), I32Or, I32Eqz]);
    body.extend(fields(0));
    body.extend([I32Const(1), ArrayGet(ARGS), RefTestNonNull(string), I32Eqz, I32Or, If(BlockType::Empty), LocalGet(0), Return, End]);
    body.extend(fields(0));
    body.extend([
        I32Const(2),
        ArrayGet(ARGS),
        LocalTee(1),
        RefTestNonNull(string),
        I32Eqz,
        If(BlockType::Empty),
        LocalGet(0),
        Return,
        End,
        // FNV-1a over the name's UTF-16 units.
        I32Const(-2128831035),
        LocalSet(2),
        I32Const(0),
        LocalSet(3),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(3),
        LocalGet(1),
        RefCastNonNull(string),
        ArrayLen,
        I32GeU,
        BrIf(1),
        LocalGet(2),
        LocalGet(1),
        RefCastNonNull(string),
        LocalGet(3),
        ArrayGetU(STRING),
        I32Xor,
        I32Const(16777619),
        I32Mul,
        LocalSet(2),
        LocalGet(3),
        I32Const(1),
        I32Add,
        LocalSet(3),
        Br(0),
        End,
        End,
        LocalGet(2),
        I32Const(KEYWORD_BUCKETS - 1),
        I32And,
        LocalSet(3),
        GlobalGet(KEYWORDS_GLOBAL),
        RefIsNull,
        If(BlockType::Empty),
        sentinel_value(NIL),
        I32Const(KEYWORD_BUCKETS),
        ArrayNew(ARGS),
        GlobalSet(KEYWORDS_GLOBAL),
        End,
        GlobalGet(KEYWORDS_GLOBAL),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        LocalTee(4),
        LocalGet(3),
        ArrayGet(ARGS),
        LocalSet(5),
        // Walk the bucket's chain for a keyword with the same name.
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(5),
        RefIsNull,
        BrIf(1),
        LocalGet(5),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        I32Const(0),
        ArrayGet(ARGS),
        LocalSet(6),
    ]);
    body.extend(fields(6));
    body.extend([
        I32Const(2),
        ArrayGet(ARGS),
        LocalSet(6),
        LocalGet(6),
        RefCastNonNull(string),
        ArrayLen,
        LocalGet(1),
        RefCastNonNull(string),
        ArrayLen,
        I32Eq,
        LocalSet(7),
        LocalGet(7),
        If(BlockType::Empty),
        I32Const(0),
        LocalSet(2),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(2),
        LocalGet(1),
        RefCastNonNull(string),
        ArrayLen,
        I32GeU,
        BrIf(1),
        LocalGet(6),
        RefCastNonNull(string),
        LocalGet(2),
        ArrayGetU(STRING),
        LocalGet(1),
        RefCastNonNull(string),
        LocalGet(2),
        ArrayGetU(STRING),
        I32Ne,
        If(BlockType::Empty),
        I32Const(0),
        LocalSet(7),
        Br(2),
        End,
        LocalGet(2),
        I32Const(1),
        I32Add,
        LocalSet(2),
        Br(0),
        End,
        End,
        End,
        LocalGet(7),
        If(BlockType::Empty),
        LocalGet(5),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        I32Const(0),
        ArrayGet(ARGS),
        Return,
        End,
        LocalGet(5),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        I32Const(1),
        ArrayGet(ARGS),
        LocalSet(5),
        Br(0),
        End,
        End,
        // The first of its name: chain it at the bucket's head, and remember
        // the keywords' descriptor.
        LocalGet(0),
        RefCastNonNull(HeapType::Concrete(7)),
        StructGet { struct_type_index: 7, field_index: 0 },
        GlobalSet(KEYWORD_DESCRIPTOR_GLOBAL),
        LocalGet(4),
        LocalGet(3),
        LocalGet(0),
        LocalGet(4),
        LocalGet(3),
        ArrayGet(ARGS),
        ArrayNewFixed { array_type_index: ARGS, array_size: 2 },
        ArraySet(ARGS),
        LocalGet(0),
    ]);
    b.function_with_locals(
        "keyword-intern",
        &[VALUE],
        &[VALUE],
        &[(1, VALUE), (2, ValType::I32), (1, reference(ARGS)), (2, VALUE), (1, ValType::I32)],
        &body,
    )
}
/// `arity-throw(closure, count)`: throws Clojure's arity error, "Wrong number
/// of args (count) passed to: name", with the arity descriptor. The name is the
/// code struct's, else the closure's display name, else "fn". Self-contained,
/// as it precedes the runtime's string functions.
fn arity_throw(b: &mut Builder) -> u32 {
    use Instruction::*;
    let string = HeapType::Concrete(STRING);
    let constant = |text: &str| {
        let units: Vec<_> = text.encode_utf16().collect();
        let mut code: Vec<Instruction<'static>> = units.iter().map(|unit| I32Const(*unit as i32)).collect();
        code.push(ArrayNewFixed { array_type_index: STRING, array_size: units.len() as u32 });
        code
    };
    // Params: 0 closure, 1 count. Locals: 2 name, 3 digits, 4 first digit,
    // 5 remaining count, 6 message, 7 write position, 8 prefix, 9 middle.
    let mut body = vec![sentinel_value(NIL), LocalSet(2)];
    body.extend([
        LocalGet(0),
        RefTestNonNull(HeapType::Concrete(CLOSURE)),
        If(BlockType::Empty),
        LocalGet(0),
        RefCastNonNull(HeapType::Concrete(CLOSURE)),
        StructGet { struct_type_index: CLOSURE, field_index: CLOSURE_CODE },
        StructGet { struct_type_index: CODE, field_index: CODE_NAME },
        LocalSet(2),
        LocalGet(2),
        RefTestNonNull(string),
        I32Eqz,
        If(BlockType::Empty),
        LocalGet(0),
        RefCastNonNull(HeapType::Concrete(CLOSURE)),
        StructGet { struct_type_index: CLOSURE, field_index: CLOSURE_PROPERTIES },
        LocalSet(2),
        LocalGet(2),
        RefTestNonNull(HeapType::Concrete(ARGS)),
        If(BlockType::Empty),
        LocalGet(2),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        I32Const(2),
        ArrayGet(ARGS),
        LocalSet(2),
        End,
        End,
        End,
        // No name, or an anonymous function's empty display name: "fn".
        LocalGet(2),
        RefTestNonNull(string),
        If(BlockType::Result(ValType::I32)),
        LocalGet(2),
        RefCastNonNull(string),
        ArrayLen,
        I32Eqz,
        Else,
        I32Const(1),
        End,
        If(BlockType::Empty),
    ]);
    body.extend(constant("fn"));
    body.extend([LocalSet(2), End]);
    // The count's decimal digits, from the end of a ten-unit buffer.
    body.extend([
        I32Const(10),
        ArrayNewDefault(STRING),
        LocalSet(3),
        I32Const(10),
        LocalSet(4),
        LocalGet(1),
        LocalSet(5),
        Loop(BlockType::Empty),
        LocalGet(4),
        I32Const(1),
        I32Sub,
        LocalSet(4),
        LocalGet(3),
        LocalGet(4),
        LocalGet(5),
        I32Const(10),
        I32RemU,
        I32Const('0' as i32),
        I32Add,
        ArraySet(STRING),
        LocalGet(5),
        I32Const(10),
        I32DivU,
        LocalTee(5),
        BrIf(0),
        End,
    ]);
    body.extend(constant("Wrong number of args ("));
    body.push(LocalSet(8));
    body.extend(constant(") passed to: "));
    body.push(LocalSet(9));
    // Length: prefix, digits, middle, name.
    body.extend([
        LocalGet(8), ArrayLen,
        I32Const(10), LocalGet(4), I32Sub, I32Add,
        LocalGet(9), ArrayLen, I32Add,
        LocalGet(2), RefCastNonNull(string), ArrayLen, I32Add,
        ArrayNewDefault(STRING),
        LocalSet(6),
        I32Const(0),
        LocalSet(7),
    ]);
    // Copy each piece at the write position: (source, start, length).
    let pieces: [(Vec<Instruction<'static>>, Vec<Instruction<'static>>, Vec<Instruction<'static>>); 4] = [
        (vec![LocalGet(8)], vec![I32Const(0)], vec![LocalGet(8), ArrayLen]),
        (vec![LocalGet(3)], vec![LocalGet(4)], vec![I32Const(10), LocalGet(4), I32Sub]),
        (vec![LocalGet(9)], vec![I32Const(0)], vec![LocalGet(9), ArrayLen]),
        (vec![LocalGet(2), RefCastNonNull(string)], vec![I32Const(0)], vec![LocalGet(2), RefCastNonNull(string), ArrayLen]),
    ];
    for (source, start, length) in pieces {
        body.extend([LocalGet(6), LocalGet(7)]);
        body.extend(source);
        body.extend(start);
        body.extend(length.clone());
        body.push(ArrayCopy { array_type_index_dst: STRING, array_type_index_src: STRING });
        body.push(LocalGet(7));
        body.extend(length);
        body.extend([I32Add, LocalSet(7)]);
    }
    body.extend([
        GlobalGet(0),
        LocalGet(6),
        RefAsNonNull,
        sentinel_value(NIL),
        sentinel_value(NIL),
        I32Const(0), // Identity UID, unassigned.
        RefI31,
        StructNew(8),
        Throw(0),
    ]);
    b.function_with_locals(
        "arity-throw",
        &[VALUE, ValType::I32],
        &[VALUE],
        &[
            (1, VALUE),
            (1, ValType::Ref(RefType { nullable: true, heap_type: string })),
            (2, ValType::I32),
            (1, ValType::Ref(RefType { nullable: true, heap_type: string })),
            (1, ValType::I32),
            (2, ValType::Ref(RefType { nullable: true, heap_type: string })),
        ],
        &body,
    )
}
/// Versions checked before any fragment initialization. Layout compatibility is
/// additionally checked by Wasm validation/linking; a manifest is not a proof.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manifest {
    pub runtime_abi: u32,
    pub compiler: String,
    pub wasm_tools: String,
}
impl Default for Manifest {
    fn default() -> Self {
        Self {
            runtime_abi: VERSION,
            compiler: format!(
                "{}+portable.{PORTABLE_FORMAT_VERSION}",
                env!("CARGO_PKG_VERSION")
            ),
            wasm_tools: "0.258.0".into(),
        }
    }
}
impl Manifest {
    pub fn section(&self) -> CustomSection<'static> {
        let mut data = self.runtime_abi.to_le_bytes().to_vec();
        data.extend_from_slice(self.compiler.as_bytes());
        data.push(0);
        data.extend_from_slice(self.wasm_tools.as_bytes());
        data.push(0);
        CustomSection {
            name: Cow::Borrowed(MANIFEST),
            data: Cow::Owned(data),
        }
    }
}

/// Reject absent, duplicate, malformed or incompatible manifests before loading
/// the artifact. Callers must then validate/link before starting guest code.
pub fn verify_artifact(bytes: &[u8], expected: &Manifest) -> Result<(), String> {
    let mut found = None;
    let mut saw_types = false;
    let mut prelude_matches = false;
    for payload in wasmparser::Parser::new(0).parse_all(bytes) {
        match payload.map_err(|e| e.to_string())? {
            wasmparser::Payload::Version {
                encoding: wasmparser::Encoding::Component,
                ..
            } => return Err("Shared runtime ABI requires a core module".into()),
            wasmparser::Payload::CustomSection(section) if section.name() == MANIFEST => {
                if found.is_some() {
                    return Err("duplicate runtime ABI manifest".into());
                }
                let data = section.data();
                if data.len() < 4 || data.len() > 4096 {
                    return Err("malformed runtime ABI manifest".into());
                }
                let runtime_abi = u32::from_le_bytes(data[..4].try_into().unwrap());
                let strings =
                    std::str::from_utf8(&data[4..]).map_err(|_| "invalid manifest text")?;
                let fields: Vec<_> = strings.split('\0').collect();
                if fields.len() != 3
                    || fields[0].is_empty()
                    || fields[1].is_empty()
                    || !fields[2].is_empty()
                {
                    return Err("malformed runtime ABI versions".into());
                }
                found = Some(Manifest {
                    runtime_abi,
                    compiler: fields[0].into(),
                    wasm_tools: fields[1].into(),
                });
            }
            wasmparser::Payload::TypeSection(reader) => {
                if saw_types {
                    return Err("duplicate runtime ABI type section".into());
                }
                saw_types = true;
                if let Some(group) = reader.into_iter().next() {
                    let group = group.map_err(|e| e.to_string())?;
                    prelude_matches = group.is_explicit_rec_group()
                        && group.types().eq(expected_prelude().iter());
                }
            }
            _ => (),
        }
    }
    match found {
        None => Err("missing runtime ABI manifest".into()),
        Some(actual) if &actual != expected => Err(format!(
            "runtime ABI version mismatch: expected {expected:?}, actual {actual:?}"
        )),
        Some(_) if !prelude_matches => Err("missing or incompatible runtime ABI prelude".into()),
        Some(_) => Ok(()),
    }
}

fn expected_prelude() -> &'static Vec<wasmparser::SubType> {
    static TYPES: std::sync::OnceLock<Vec<wasmparser::SubType>> = std::sync::OnceLock::new();
    TYPES.get_or_init(|| {
        let mut module = Module::new();
        module.section(&prelude());
        let bytes = module.finish();
        for payload in wasmparser::Parser::new(0).parse_all(&bytes) {
            if let wasmparser::Payload::TypeSection(reader) =
                payload.expect("generated ABI prelude")
            {
                return reader
                    .into_iter()
                    .next()
                    .unwrap()
                    .expect("generated recursive group")
                    .types()
                    .cloned()
                    .collect();
            }
        }
        unreachable!("generated runtime always has its type prelude")
    })
}

struct Builder {
    types: TypeSection,
    functions: FunctionSection,
    exports: ExportSection,
    code: CodeSection,
    count: u32,
    next_type: u32,
    names: std::collections::BTreeMap<String, u32>,
}
impl Builder {
    fn function(
        &mut self,
        name: &str,
        params: &[ValType],
        results: &[ValType],
        instructions: &[Instruction<'_>],
    ) -> u32 {
        self.function_with_locals(name, params, results, &[], instructions)
    }
    /// A private function of one of the shared group's function types, such as
    /// an entry `$callk`. Not exported.
    fn typed(&mut self, ty: u32, locals: &[(u32, ValType)], instructions: &[Instruction<'_>]) -> u32 {
        let index = self.count;
        self.functions.function(ty);
        let mut function = Function::new(locals.iter().copied());
        for instruction in instructions {
            function.instruction(instruction);
        }
        function.instruction(&Instruction::End);
        self.code.function(&function);
        self.count += 1;
        index
    }
    fn function_with_locals(
        &mut self,
        name: &str,
        params: &[ValType],
        results: &[ValType],
        locals: &[(u32, ValType)],
        instructions: &[Instruction<'_>],
    ) -> u32 {
        let index = self.count;
        self.names.insert(name.to_owned(), index);
        self.types
            .ty()
            .function(params.iter().copied(), results.iter().copied());
        self.functions.function(self.next_type);
        self.next_type += 1;
        self.exports.export(name, ExportKind::Func, self.count);
        let mut function = Function::new(locals.iter().copied());
        for instruction in instructions {
            function.instruction(instruction);
        }
        function.instruction(&Instruction::End);
        self.code.function(&function);
        self.count += 1;
        index
    }
}

/// Generate the production core runtime, without target imports. Private numeric
/// conversion scratch memory is separate from GC language storage.
/// These storage/numeric intrinsics are for verified lowering, not user APIs.
/// Generic invocation checks arity centrally and throws a language error.
/// Target adapters and compiler lowering remain separate.
pub fn module() -> Vec<u8> {
    static BYTES: std::sync::OnceLock<Vec<u8>> = std::sync::OnceLock::new();
    BYTES.get_or_init(build_module).clone()
}
fn build_module() -> Vec<u8> {
    use Instruction::*;
    let helper = numeric::helper();
    let numeric_info = helper.info();
    let mut b = Builder {
        types: helper.types,
        functions: helper.functions,
        exports: ExportSection::new(),
        code: helper.code,
        count: helper.function_count,
        next_type: helper.type_count,
        names: std::collections::BTreeMap::new(),
    };
    let string = HeapType::Concrete(STRING);
    for (name, sentinel) in [("nil", NIL), ("false", FALSE), ("true", TRUE)] {
        b.function(name, &[], &[VALUE], &[sentinel_value(sentinel)]);
    }
    b.function(
        "number-box",
        &[ValType::F64],
        &[VALUE],
        &[LocalGet(0), StructNew(NUMBER)],
    );
    longs::intrinsics(&mut b);
    longs::arithmetic(&mut b);
    longs::string(&mut b);
    longs::functions(&mut b);
    longs::parse(&mut b);
    b.function(
        "string-new",
        &[ValType::I32],
        &[VALUE],
        &[LocalGet(0), ArrayNewDefault(STRING)],
    );
    b.function(
        "string-length",
        &[VALUE],
        &[ValType::I32],
        &[LocalGet(0), RefCastNonNull(string), ArrayLen],
    );
    b.function(
        "string-unit",
        &[VALUE, ValType::I32],
        &[ValType::I32],
        &[
            LocalGet(0),
            RefCastNonNull(string),
            LocalGet(1),
            ArrayGetU(STRING),
        ],
    );
    // Checked storage helper: false rejects invalid index/unit without truncation.
    b.function(
        "string-set-unit",
        &[VALUE, ValType::I32, ValType::I32],
        &[ValType::I32],
        &[
            LocalGet(2),
            I32Const(65535),
            I32GtU,
            LocalGet(1),
            LocalGet(0),
            RefCastNonNull(string),
            ArrayLen,
            I32GeU,
            I32Or,
            If(BlockType::Result(ValType::I32)),
            I32Const(0),
            Else,
            LocalGet(0),
            RefCastNonNull(string),
            LocalGet(1),
            LocalGet(2),
            ArraySet(STRING),
            I32Const(1),
            End,
        ],
    );
    b.function(
        "args-new",
        &[ValType::I32],
        &[VALUE],
        &[sentinel_value(NIL), LocalGet(0), ArrayNew(ARGS)],
    );
    let closure_environment = closure_properties::functions(&mut b);
    let arity_throw = arity_throw(&mut b);
    // A universal closure: the shared universal code struct, no properties
    // until one is set, an unassigned UID, then its environment, function and
    // arity range.
    b.function(
        "closure-new",
        &[VALUE, reference(INVOKE), ValType::I32, ValType::I32],
        &[VALUE],
        &[
            GlobalGet(UNIVERSAL_CODE_GLOBAL),
            sentinel_value(NIL),
            I32Const(0), // Identity UID: private unassigned marker.
            RefI31,
            LocalGet(0),
            LocalGet(1),
            LocalGet(2),
            LocalGet(3),
            StructNew(UNIVERSAL),
        ],
    );
    let closure = HeapType::Concrete(UNIVERSAL);
    let args = HeapType::Concrete(ARGS);
    // Calling a name a failed `def` or `declare` left unbound is Clojure's
    // unbound-fn error, not a non-function's (#288).
    let mut invoke = vec![
        LocalGet(0),
        sentinel_value(UNDEFINED),
        RefEq,
        If(BlockType::Empty),
        GlobalGet(2),
    ];
    let units: Vec<_> = "Attempting to call unbound fn".encode_utf16().collect();
    invoke.extend(units.iter().map(|unit| I32Const(*unit as i32)));
    invoke.extend([
        ArrayNewFixed {
            array_type_index: STRING,
            array_size: units.len() as u32,
        },
        sentinel_value(NIL),
        sentinel_value(NIL),
        I32Const(0), // Identity UID, unassigned.
        RefI31,
        StructNew(8),
        Throw(0),
        End,
    ]);
    for (local, kind, descriptor, message) in [
        (0, HeapType::Concrete(CLOSURE), 2, "Not callable"),
        (1, args, 3, "Invalid argument array"),
    ] {
        invoke.extend([
            LocalGet(local),
            RefTestNonNull(kind),
            I32Eqz,
            If(BlockType::Empty),
            GlobalGet(descriptor),
        ]);
        let units: Vec<_> = message.encode_utf16().collect();
        invoke.extend(units.iter().map(|unit| I32Const(*unit as i32)));
        invoke.extend([
            ArrayNewFixed {
                array_type_index: STRING,
                array_size: units.len() as u32,
            },
            sentinel_value(NIL),
            sentinel_value(NIL),
            I32Const(0), // Identity UID, unassigned.
            RefI31,
            StructNew(8),
            Throw(0),
            End,
        ]);
    }
    // Any other closure takes its spread entry, with no unrealized rest.
    invoke.extend([
        LocalGet(0),
        RefTestNonNull(closure),
        I32Eqz,
        If(BlockType::Empty),
        LocalGet(0),
        RefCastNonNull(HeapType::Concrete(CLOSURE)),
        LocalGet(1),
        RefCastNonNull(args),
        sentinel_value(NIL),
        LocalGet(0),
        RefCastNonNull(HeapType::Concrete(CLOSURE)),
        StructGet { struct_type_index: CLOSURE, field_index: CLOSURE_CODE },
        StructGet { struct_type_index: CODE, field_index: CODE_SPREAD },
        ReturnCallRef(SPREAD),
        End,
    ]);
    invoke.extend([
        LocalGet(1),
        RefCastNonNull(args),
        ArrayLen,
        LocalGet(0),
        RefCastNonNull(closure),
        StructGet {
            struct_type_index: UNIVERSAL,
            field_index: UNIVERSAL_MIN,
        },
        I32LtU,
        LocalGet(0),
        RefCastNonNull(closure),
        StructGet {
            struct_type_index: UNIVERSAL,
            field_index: UNIVERSAL_MAX,
        },
        I32Const(-1),
        I32Ne,
        LocalGet(1),
        RefCastNonNull(args),
        ArrayLen,
        LocalGet(0),
        RefCastNonNull(closure),
        StructGet {
            struct_type_index: UNIVERSAL,
            field_index: UNIVERSAL_MAX,
        },
        I32GtU,
        I32And,
        I32Or,
        If(BlockType::Empty),
        LocalGet(0),
        LocalGet(1),
        RefCastNonNull(args),
        ArrayLen,
        Call(arity_throw),
        Drop,
        End,
        LocalGet(0),
        Call(closure_environment),
        LocalGet(1),
        RefCastNonNull(args),
        LocalGet(0),
        RefCastNonNull(closure),
        StructGet {
            struct_type_index: UNIVERSAL,
            field_index: UNIVERSAL_INVOKE,
        },
        ReturnCallRef(INVOKE),
    ]);
    let message: Vec<_> = "Wrong arity".encode_utf16().collect();
    let mut arity_error = vec![GlobalGet(0)];
    arity_error.extend(message.iter().map(|unit| I32Const(*unit as i32)));
    arity_error.extend([
        ArrayNewFixed {
            array_type_index: STRING,
            array_size: message.len() as u32,
        },
        sentinel_value(NIL),
        sentinel_value(NIL),
        I32Const(0), // Identity UID, unassigned.
        RefI31,
        StructNew(8),
        Throw(0),
    ]);
    b.function("arity-error", &[], &[VALUE], &arity_error);
    let generic_invoke = b.function("invoke", &[VALUE, VALUE], &[VALUE], &invoke);
    // The universal code struct's entries pack their arguments for `invoke`,
    // which checks the universal closure's arity range. Its spread entry takes
    // realized arguments only until `apply` passes an unrealized rest.
    let mut universal_entries = vec![];
    for k in 0..=MAX_FIXED {
        let mut body = vec![LocalGet(0)];
        body.extend((1..=k).map(LocalGet));
        body.extend([ArrayNewFixed { array_type_index: ARGS, array_size: k }, ReturnCall(generic_invoke)]);
        universal_entries.push(b.typed(CALL + k, &[], &body));
    }
    let mut body = vec![LocalGet(2), RefIsNull, I32Eqz, If(BlockType::Empty)];
    nominal::error(&mut body);
    body.extend([End, LocalGet(0), LocalGet(1), ReturnCall(generic_invoke)]);
    universal_entries.push(b.typed(SPREAD, &[], &body));
    b.names.insert("universal-spread".to_owned(), *universal_entries.last().unwrap());
    for k in 0..=MAX_FIXED {
        let index = b.typed(CALL + k, &[], &[LocalGet(0), I32Const(k as i32), ReturnCall(arity_throw)]);
        b.names.insert(ARITY_ERROR_EXPORTS[k as usize].to_owned(), index);
        b.exports.export(ARITY_ERROR_EXPORTS[k as usize], ExportKind::Func, index);
    }
    b.function(
        "binding-new",
        &[VALUE],
        &[VALUE],
        &[LocalGet(0), I32Const(1), StructNew(5)],
    );
    b.function(
        "binding-unbound",
        &[],
        &[VALUE],
        &[sentinel_value(NIL), I32Const(0), StructNew(5)],
    );
    b.function(
        "binding-bound",
        &[VALUE],
        &[VALUE],
        &[
            LocalGet(0),
            RefCastNonNull(HeapType::Concrete(5)),
            StructGet {
                struct_type_index: 5,
                field_index: 1,
            },
            I32Const(CELL_BOUND),
            I32And,
            If(BlockType::Result(VALUE)),
            sentinel_value(TRUE),
            Else,
            sentinel_value(FALSE),
            End,
        ],
    );
    let dynamic_lookup = dynamic::lookup(&mut b);
    let mut binding_get = vec![
        LocalGet(0),
        RefCastNonNull(HeapType::Concrete(5)),
        StructGet {
            struct_type_index: 5,
            field_index: 1,
        },
        I32Const(CELL_BOUND),
        I32And,
        I32Eqz,
        If(BlockType::Empty),
        GlobalGet(1),
    ];
    let unbound_message: Vec<_> = "Unbound binding".encode_utf16().collect();
    binding_get.extend(unbound_message.iter().map(|unit| I32Const(*unit as i32)));
    binding_get.extend([
        ArrayNewFixed {
            array_type_index: STRING,
            array_size: unbound_message.len() as u32,
        },
        sentinel_value(NIL),
        sentinel_value(NIL),
        I32Const(0), // Identity UID, unassigned.
        RefI31,
        StructNew(8),
        Throw(0),
        End,
        LocalGet(0),
        RefCastNonNull(HeapType::Concrete(5)),
        StructGet {
            struct_type_index: 5,
            field_index: 0,
        },
    ]);
    dynamic::binding_get(&mut b, dynamic_lookup, binding_get);
    // Anonymous publication tests the same selected dynamic/root cell value
    // as an ordinary read. An unbound root must not be read eagerly, while an
    // active dynamic entry may still provide a value for that root cell.
    b.function_with_locals(
        "binding-defined",
        &[VALUE],
        &[ValType::I32],
        &[(1, VALUE), (1, ValType::I32)],
        &[
            LocalGet(0),
            GlobalGet(dynamic::CURRENT),
            Call(dynamic_lookup),
            LocalSet(2),
            LocalSet(1),
            LocalGet(2),
            I32Const(0),
            I32GeS,
            If(BlockType::Result(ValType::I32)),
            LocalGet(1),
            RefCastNonNull(HeapType::Concrete(ARGS)),
            LocalGet(2),
            ArrayGet(ARGS),
            sentinel_value(UNDEFINED),
            RefEq,
            I32Eqz,
            Else,
            LocalGet(0),
            Call(b.names["binding-bound"]),
            sentinel_value(TRUE),
            RefEq,
            If(BlockType::Result(ValType::I32)),
            LocalGet(0),
            RefCastNonNull(HeapType::Concrete(5)),
            StructGet {
                struct_type_index: 5,
                field_index: 0,
            },
            sentinel_value(UNDEFINED),
            RefEq,
            I32Eqz,
            Else,
            I32Const(0),
            End,
            End,
        ],
    );
    let binding_set = dynamic::binding_set(&mut b, dynamic_lookup);
    dynamic::switching(&mut b);
    let coercion_types = coercions::declare(&mut b);
    let prototype_types = owned_prototypes::declare(&mut b);
    error_constructors::canonical_test_function(&mut b);
    numeric::intrinsics(&mut b, numeric_info, coercion_types);
    let primitives = numeric_operands::intrinsics(&mut b);
    bitwise::intrinsics(&mut b);
    numeric_hash::intrinsics(&mut b);
    identity_hash::intrinsics(&mut b);
    // Casts come before arrays, as packed primitive arrays convert with them.
    let bitwise_functions = bitwise::functions(&mut b);
    packed_arrays::functions(&mut b);
    let mut arithmetic_functions = arithmetic::functions(&mut b, primitives);
    sparse_arrays::functions(&mut b);
    arithmetic_functions.extend(arrays::functions(&mut b));
    arithmetic_functions.extend(nominal::functions(&mut b, generic_invoke));
    // The out-of-line call path (ADR-0004). A closure is invoked; any other
    // callee is bound through IFn's `-invoke` key for its signature arity,
    // which core registers as it declares IFn, as the call site's own
    // protocol path did; without a key, `invoke` reports Clojure's
    // unbound-fn or not-callable error.
    let keys = CALLABLE_KEYS_GLOBAL;
    let mut body = vec![
        LocalGet(1),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        ArrayLen,
        I32Const(1),
        I32Add,
        LocalSet(2),
        LocalGet(0),
        RefTestNonNull(HeapType::Concrete(CLOSURE)),
        GlobalGet(keys),
        RefIsNull,
        I32Or,
        LocalGet(2),
        I32Const(CALLABLE_KEY_COUNT),
        I32GeU,
        I32Or,
        If(BlockType::Empty),
        LocalGet(0),
        LocalGet(1),
        ReturnCall(generic_invoke),
        End,
        GlobalGet(keys),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        LocalGet(2),
        ArrayGet(ARGS),
        LocalTee(3),
        RefIsNull,
        If(BlockType::Empty),
        LocalGet(0),
        LocalGet(1),
        ReturnCall(generic_invoke),
        End,
        LocalGet(0),
        LocalGet(3),
        Call(b.names["callable-bind"]),
        LocalGet(1),
        ReturnCall(generic_invoke),
    ];
    let call_spread = b.function_with_locals("call-spread", &[VALUE, VALUE], &[VALUE], &[(1, ValType::I32), (1, VALUE)], &body);
    for k in 0..=MAX_FIXED {
        let mut body: Vec<_> = (0..=k).map(LocalGet).collect();
        body.extend([ArrayNewFixed { array_type_index: ARGS, array_size: k }, ReturnCall(call_spread)]);
        b.function(CALL_EXPORTS[k as usize], &vec![VALUE; k as usize + 1], &[VALUE], &body);
    }
    // `(defprotocol IFn …)` registers `-invoke`'s key for each signature arity.
    body = vec![
        GlobalGet(keys),
        RefIsNull,
        If(BlockType::Empty),
        sentinel_value(NIL),
        I32Const(CALLABLE_KEY_COUNT),
        ArrayNew(ARGS),
        GlobalSet(keys),
        End,
        LocalGet(0),
        RefTestNonNull(HeapType::I31),
        I32Eqz,
        If(BlockType::Empty),
    ];
    nominal::error(&mut body);
    body.extend([
        End,
        LocalGet(0),
        RefCastNonNull(HeapType::I31),
        I31GetU,
        LocalTee(2),
        I32Const(CALLABLE_KEY_COUNT),
        I32GeU,
        If(BlockType::Empty),
    ]);
    nominal::error(&mut body);
    body.extend([
        End,
        GlobalGet(keys),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        LocalGet(2),
        LocalGet(1),
        ArraySet(ARGS),
        sentinel_value(NIL),
    ]);
    b.function_with_locals("callable-key-register", &[VALUE, VALUE], &[VALUE], &[(1, ValType::I32)], &body);
    keyword_intern(&mut b);
    b.function(
        "keyword-test",
        &[VALUE],
        &[ValType::I32],
        &[
            LocalGet(0),
            RefTestNonNull(HeapType::Concrete(7)),
            If(BlockType::Result(ValType::I32)),
            LocalGet(0),
            RefCastNonNull(HeapType::Concrete(7)),
            StructGet { struct_type_index: 7, field_index: 0 },
            GlobalGet(KEYWORD_DESCRIPTOR_GLOBAL),
            RefEq,
            Else,
            I32Const(0),
            End,
        ],
    );
    let try_invoke = exceptions::functions(&mut b, generic_invoke);
    arithmetic_functions.extend(dynamic::functions(&mut b, binding_set, try_invoke));
    arithmetic_functions.extend(predicates::functions(&mut b));
    arithmetic_functions.extend(comparisons::functions(&mut b, coercion_types));
    arithmetic_functions.extend(bitwise_functions);
    arithmetic_functions.extend(named_properties::functions(&mut b));
    // `named-field-slot(object, name)`: the index of a nominal object's field
    // with this name in its descriptor's schema, or -1 (#361). Call sites
    // cache it per descriptor, as a schema never changes.
    let mut body = vec![
        LocalGet(0),
        RefTestNonNull(HeapType::Concrete(7)),
        I32Eqz,
        If(BlockType::Empty),
        I32Const(-1),
        Return,
        End,
        LocalGet(0),
        RefCastNonNull(HeapType::Concrete(7)),
        StructGet { struct_type_index: 7, field_index: 0 },
        StructGet { struct_type_index: DESCRIPTOR, field_index: 1 },
        LocalTee(2),
        RefTestNonNull(HeapType::Concrete(ARGS)),
        I32Eqz,
        If(BlockType::Empty),
        I32Const(-1),
        Return,
        End,
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(3),
        LocalGet(2),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        ArrayLen,
        I32GeU,
        BrIf(1),
        LocalGet(2),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        LocalGet(3),
        ArrayGet(ARGS),
        LocalGet(1),
        Call(b.names["property-key-equal"]),
        If(BlockType::Empty),
        LocalGet(3),
        Return,
        End,
        LocalGet(3),
        I32Const(1),
        I32Add,
        LocalSet(3),
        Br(0),
        End,
        End,
        I32Const(-1),
    ];
    b.function_with_locals("named-field-slot", &[VALUE, VALUE], &[ValType::I32], &[(1, VALUE), (1, ValType::I32)], &body);
    // Original source error adapter: retain the ABI Error descriptor/message
    // rather than substituting nil, an opaque object or a Wasm trap.
    b.function(
        "language-error-new",
        &[VALUE],
        &[VALUE],
        &[
            GlobalGet(nominal::ERROR_GLOBAL),
            LocalGet(0),
            Call(b.names["coerce-string"]),
            RefCastNonNull(HeapType::Concrete(STRING)),
            sentinel_value(NIL),
            sentinel_value(NIL),
            I32Const(0), // Identity UID, unassigned.
            RefI31,
            StructNew(8),
        ],
    );
    // Native Error adapters use rooted descriptor identity. A structurally
    // identical payload with another descriptor is not a language Error.
    let mut error_test = vec![];
    error_constructors::family_test(&b, &mut error_test, 0);
    b.function("language-error-is", &[VALUE], &[ValType::I32], &error_test);
    native_objects::functions(&mut b);
    native_object_properties::primitives(&mut b);
    native_object_properties::functions(&mut b);
    // After named-property-get, which native_object_properties defines.
    named_site_get(&mut b);
    arrays::set_or_property(&mut b);
    arithmetic_functions.extend(native_object_methods::functions(&mut b));
    arithmetic_functions.extend(native_object_factory::functions(&mut b));
    r#async::intrinsics(&mut b);
    arithmetic_functions.extend(async_library::functions(&mut b));
    arithmetic_functions.extend(primitive_constructors::functions(&mut b));
    let error_callback = error_constructors::callback(&mut b);
    arithmetic_functions.push(error_callback);
    arithmetic_functions.extend(range_errors::functions(&mut b, error_callback));
    error_constructors::functions(&mut b, error_callback);
    arithmetic_functions.extend(error_properties::functions(&mut b));
    let (object_descriptor_functions, object_initialize) = object_descriptors::functions(&mut b);
    arithmetic_functions.extend(object_descriptor_functions);
    arithmetic_functions.extend(exception_info::functions(&mut b));
    type_values::functions(&mut b);
    let prototype_functions = owned_prototypes::functions(&mut b);
    instance_site_test(&mut b);
    arithmetic_functions.extend(prototype_functions);
    let coercion_functions = coercions::functions(&mut b);
    arithmetic_functions.extend(coercion_functions);
    let mut elements = ElementSection::new();
    elements.declared(Elements::Functions(Cow::Owned(arithmetic_functions)));
    let mut tags = TagSection::new();
    tags.tag(TagType {
        kind: TagKind::Exception,
        func_type_idx: b.next_type,
    });
    b.types.ty().function([VALUE], []);
    // The Macro observer appends a setter after the exception-tag signature.
    b.next_type += 1;
    let mut globals = GlobalSection::new();
    // Separate rooted descriptors: arity, unbound, not callable, invalid arguments.
    for identity in 1..=numeric::ERROR_GLOBALS {
        globals.global(
            GlobalType {
                val_type: reference(DESCRIPTOR),
                mutable: false,
                shared: false,
            },
            &ConstExpr::extended([
                I64Const(identity.into()),
                sentinel_value(NIL),
                sentinel_value(NIL),
                sentinel_value(NIL),
                I32Const(0), // Identity UID, unassigned.
                RefI31,
                sentinel_value(NIL), StructNew(DESCRIPTOR),
            ]),
        );
    }
    globals.global(
        GlobalType {
            val_type: ValType::I32,
            mutable: true,
            shared: false,
        },
        &ConstExpr::i32_const(numeric_info.stack_top),
    );
    assert_eq!(globals.len(), FALSE_GLOBAL);
    for (name, tag) in SENTINEL_EXPORTS {
        globals.global(
            GlobalType {
                val_type: reference(SENTINEL),
                mutable: false,
                shared: false,
            },
            &ConstExpr::extended([I32Const(tag), StructNew(SENTINEL)]),
        );
        b.exports.export(name, ExportKind::Global, globals.len() - 1);
    }
    // Append after the pinned numeric stack global; never relocate its index.
    globals.global(
        GlobalType {
            val_type: ValType::I64,
            mutable: true,
            shared: false,
        },
        &ConstExpr::i64_const(i64::from(numeric::ERROR_GLOBALS) + 5),
    );
    globals.global(
        GlobalType {
            val_type: reference(DESCRIPTOR),
            mutable: false,
            shared: false,
        },
        &ConstExpr::extended([
            I64Const(i64::from(numeric::ERROR_GLOBALS) + 1),
            I32Const(0),
            ArrayNewDefault(ARGS),
            I32Const(0),
            ArrayNewDefault(ARGS),
            sentinel_value(NIL),
            I32Const(0), // Identity UID, unassigned.
            RefI31,
            sentinel_value(NIL), StructNew(DESCRIPTOR),
        ]),
    );
    // A rooted opaque protocol marker, distinct from booleans and callables.
    globals.global(
        GlobalType {
            val_type: reference(DESCRIPTOR),
            mutable: false,
            shared: false,
        },
        &ConstExpr::extended([
            I64Const(0),
            sentinel_value(NIL),
            sentinel_value(NIL),
            sentinel_value(NIL),
            I32Const(0), // Identity UID, unassigned.
            RefI31,
            sentinel_value(NIL), StructNew(DESCRIPTOR),
        ]),
    );
    globals.global(
        GlobalType {
            val_type: VALUE,
            mutable: true,
            shared: false,
        },
        &ConstExpr::extended([sentinel_value(NIL)]),
    );
    let mut info_descriptor = vec![I64Const(i64::from(numeric::ERROR_GLOBALS) + 2)];
    for field in ["message", "data", "cause"] {
        let units: Vec<_> = field.encode_utf16().collect();
        info_descriptor.extend(units.iter().map(|x| I32Const(*x as i32)));
        info_descriptor.push(ArrayNewFixed {
            array_type_index: STRING,
            array_size: units.len() as u32,
        });
    }
    info_descriptor.extend([
        ArrayNewFixed {
            array_type_index: ARGS,
            array_size: 3,
        },
        I32Const(0),
        ArrayNewDefault(ARGS),
    ]);
    // The type's name, as compiled descriptors carry theirs (#304).
    let name: Vec<_> = "suss.core.ExceptionInfo".encode_utf16().collect();
    info_descriptor.extend(name.iter().map(|x| I32Const(*x as i32)));
    info_descriptor.extend([
        ArrayNewFixed {
            array_type_index: STRING,
            array_size: name.len() as u32,
        },
        I32Const(0), // Identity UID, unassigned.
        RefI31,
        sentinel_value(NIL), StructNew(DESCRIPTOR),
    ]);
    globals.global(
        GlobalType {
            val_type: reference(DESCRIPTOR),
            mutable: false,
            shared: false,
        },
        &ConstExpr::extended(info_descriptor),
    );
    let mut ordinary_root = vec![I64Const(i64::from(numeric::ERROR_GLOBALS) + 3)];
    for field in ["message", "data", "cause"] {
        let units: Vec<_> = field.encode_utf16().collect();
        ordinary_root.extend(units.iter().map(|x| I32Const(*x as i32)));
        ordinary_root.push(ArrayNewFixed {
            array_type_index: STRING,
            array_size: units.len() as u32,
        });
    }
    ordinary_root.extend([
        ArrayNewFixed {
            array_type_index: ARGS,
            array_size: 3,
        },
        I32Const(0),
        ArrayNewDefault(ARGS),
        sentinel_value(NIL),
        I32Const(0), // Identity UID, unassigned.
        RefI31,
        sentinel_value(NIL), StructNew(DESCRIPTOR),
        sentinel_value(UNDEFINED),
        I32Const(3),
        ArrayNew(ARGS),
        sentinel_value(NIL),
        I32Const(0), // Identity UID, unassigned.
        RefI31,
        StructNew(7),
    ]);
    globals.global(
        GlobalType {
            val_type: reference(7),
            mutable: false,
            shared: false,
        },
        &ConstExpr::extended(ordinary_root),
    );
    // Owner tag for closure environments/properties; identity is reference based.
    globals.global(
        GlobalType {
            val_type: reference(DESCRIPTOR),
            mutable: false,
            shared: false,
        },
        &ConstExpr::extended([
            I64Const(0),
            I32Const(0),
            ArrayNewDefault(ARGS),
            I32Const(0),
            ArrayNewDefault(ARGS),
            sentinel_value(NIL),
            I32Const(0), // Identity UID, unassigned.
            RefI31,
            sentinel_value(NIL), StructNew(DESCRIPTOR),
        ]),
    );
    // Private source-array identity; appended without changing old global indices.
    globals.global(
        GlobalType {
            val_type: reference(DESCRIPTOR),
            mutable: false,
            shared: false,
        },
        &ConstExpr::extended([
            I64Const(0),
            I32Const(0),
            ArrayNewDefault(ARGS),
            I32Const(0),
            ArrayNewDefault(ARGS),
            sentinel_value(NIL),
            I32Const(0), // Identity UID, unassigned.
            RefI31,
            sentinel_value(NIL), StructNew(DESCRIPTOR),
        ]),
    );
    // Appended private Object method tag and portable implicit-this realm.
    globals.global(
        GlobalType {
            val_type: reference(DESCRIPTOR),
            mutable: false,
            shared: false,
        },
        &ConstExpr::extended([
            I64Const(0),
            I32Const(0),
            ArrayNewDefault(ARGS),
            I32Const(0),
            ArrayNewDefault(ARGS),
            sentinel_value(NIL),
            I32Const(0), // Identity UID, unassigned.
            RefI31,
            sentinel_value(NIL), StructNew(DESCRIPTOR),
        ]),
    );
    globals.global(
        GlobalType {
            val_type: reference(7),
            mutable: false,
            shared: false,
        },
        &ConstExpr::extended([
            I64Const(0),
            I32Const(0),
            ArrayNewDefault(ARGS),
            I32Const(0),
            ArrayNewDefault(ARGS),
            sentinel_value(NIL),
            I32Const(0), // Identity UID, unassigned.
            RefI31,
            sentinel_value(NIL), StructNew(DESCRIPTOR),
            I32Const(0),
            ArrayNewDefault(ARGS),
            sentinel_value(NIL),
            I32Const(0), // Identity UID, unassigned.
            RefI31,
            StructNew(7),
        ]),
    );
    // Private singleton builtin method root; no owners or per-string registry.
    globals.global(
        GlobalType {
            val_type: VALUE,
            mutable: true,
            shared: false,
        },
        // lazy-root absent marker, compared with nil on first use.
        &ConstExpr::extended([sentinel_value(NIL)]),
    );
    // Original owned dynamic data-property storage tag, appended privately.
    globals.global(
        GlobalType {
            val_type: reference(DESCRIPTOR),
            mutable: false,
            shared: false,
        },
        &ConstExpr::extended([
            I64Const(0),
            I32Const(0),
            ArrayNewDefault(ARGS),
            I32Const(0),
            ArrayNewDefault(ARGS),
            sentinel_value(NIL),
            I32Const(0), // Identity UID, unassigned.
            RefI31,
            sentinel_value(NIL), StructNew(DESCRIPTOR),
        ]),
    );
    // Lazy shared default Object prototype, owned by the runtime GC root.
    globals.global(
        GlobalType {
            val_type: VALUE,
            mutable: true,
            shared: false,
        },
        // lazy-root absent marker, compared with nil on first use.
        &ConstExpr::extended([sentinel_value(NIL)]),
    );
    // Shared Array.slice function root; no receiver retained.
    globals.global(
        GlobalType {
            val_type: VALUE,
            mutable: true,
            shared: false,
        },
        // lazy-root absent marker, compared with nil on first use.
        &ConstExpr::extended([sentinel_value(NIL)]),
    );
    // Scalar-only UID allocator; no global reference table retains owners.
    globals.global(
        GlobalType {
            val_type: ValType::I64,
            mutable: true,
            shared: false,
        },
        &ConstExpr::i64_const(1),
    );
    r#async::append_descriptor(&mut globals);
    streams::append_globals(&mut globals, r#async::STREAM_GLOBAL_BASE);
    type_values::append_globals(&mut globals);
    primitive_constructors::append_globals(&mut globals);
    range_errors::append_globals(&mut globals);
    array_own_properties::append_globals(&mut globals);
    coercions::append_globals(&mut globals, coercion_types, coercion_functions);
    array_join::append_globals(&mut globals);
    string_char_at::append_globals(&mut globals);
    owned_prototypes::append_globals(
        &mut globals,
        prototype_types,
        prototype_functions,
        object_initialize,
        b.names["owned-type-error-implementation"],
    );
    assert_eq!(globals.len(), object_methods::CURRENT_THIS);
    // Append without relocating any existing runtime global or descriptor ID.
    globals.global(
        GlobalType {
            val_type: VALUE,
            mutable: true,
            shared: false,
        },
        &ConstExpr::extended([GlobalGet(object_methods::DEFAULT_THIS)]),
    );
    assert_eq!(globals.len(), UNIVERSAL_CODE_GLOBAL);
    let mut code = vec![sentinel_value(NIL)];
    code.extend(universal_entries.iter().map(|entry| RefFunc(*entry)));
    // A universal closure holds its own arity range.
    code.extend([I32Const(0), I32Const(-1), StructNew(CODE)]);
    globals.global(
        GlobalType {
            val_type: reference(CODE),
            mutable: false,
            shared: false,
        },
        &ConstExpr::extended(code),
    );
    // IFn's registered `-invoke` keys, by signature arity, or null.
    globals.global(
        GlobalType {
            val_type: VALUE,
            mutable: true,
            shared: false,
        },
        &ConstExpr::extended([sentinel_value(NIL)]),
    );
    // Protocol dispatchers' code struct (#355): typed entries that dispatch
    // and call the implementation's entry directly.
    assert_eq!(globals.len(), DISPATCH_CODE_GLOBAL);
    let mut code = vec![sentinel_value(NIL), RefFunc(b.names[ARITY_ERROR_EXPORTS[0]])];
    code.extend((1..=MAX_FIXED).map(|k| RefFunc(b.names[&format!("protocol-dispatch-{k}")])));
    code.extend([RefFunc(b.names["universal-spread"]), I32Const(0), I32Const(-1), StructNew(CODE)]);
    globals.global(
        GlobalType {
            val_type: reference(CODE),
            mutable: false,
            shared: false,
        },
        &ConstExpr::extended(code),
    );
    // Interned keywords (#361), created on first use, and their descriptor.
    assert_eq!(globals.len(), DISPATCH_CODE_GLOBAL + 1);
    for _ in 0..2 {
        globals.global(
            GlobalType { val_type: VALUE, mutable: true, shared: false },
            &ConstExpr::extended([sentinel_value(NIL)]),
        );
    }
    assert_eq!(globals.len(), PROTOCOL_EPOCH_GLOBAL);
    globals.global(
        GlobalType { val_type: ValType::I32, mutable: true, shared: false },
        &ConstExpr::i32_const(0),
    );
    b.exports.export("protocol-epoch", ExportKind::Global, PROTOCOL_EPOCH_GLOBAL);
    b.exports.export("array-join-active", ExportKind::Global, array_join::ACTIVE);
    b.exports
        .export("dynamic-frame", ExportKind::Global, dynamic::CURRENT);
    b.exports
        .export("numeric-scratch-memory", ExportKind::Memory, 0);
    b.exports.export(
        "numeric-stack-pointer",
        ExportKind::Global,
        numeric_info.stack_global,
    );
    b.exports.export("language-exception", ExportKind::Tag, 0);
    let mut runtime = Module::new();
    runtime
        .section(&b.types)
        .section(&b.functions)
        .section(&helper.memory)
        .section(&tags)
        .section(&globals)
        .section(&b.exports)
        .section(&elements)
        .section(&b.code)
        .section(&helper.data)
        .section(&Manifest::default().section());
    runtime.finish()
}
