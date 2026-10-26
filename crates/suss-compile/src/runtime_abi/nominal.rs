//! Original checked nominal storage primitives for the shared ABI.
//! Internal arrays are schemas/fields/tables, not language persistent collections.
use super::*;

pub(super) const ID_GLOBAL: u32 = numeric::ERROR_GLOBALS + 1;
pub(super) const ERROR_GLOBAL: u32 = ID_GLOBAL + 1;
pub(super) const SENTINEL_GLOBAL: u32 = ERROR_GLOBAL + 1;
const OBJECT: u32 = 7;

/// Set in the identity of descriptors created by the shipped core bootstrap:
/// their instance fields and class properties are private to core.
pub(super) const CORE_PRIVATE_BIT: i64 = 1 << 62;
/// With CORE_PRIVATE_BIT: user-facing collection classes whose whitelisted
/// static members (EMPTY, fromArray, ...) user code may read.
pub(super) const CORE_PUBLIC_STATICS_BIT: i64 = 1 << 61;
/// Every descriptor created by privileged source: its core protocols and Object
/// methods are closed to user extension, even where its fields are public.
pub(super) const CORE_CLOSED_BIT: i64 = 1 << 60;
/// Runtime-built descriptors (errors, ExceptionInfo, ...) have fixed identities
/// below the first counter-assigned one; they are closed to extension too.
pub(super) const FIRST_COUNTED_IDENTITY: i64 = numeric::ERROR_GLOBALS as i64 + 5;

pub(super) fn error(body: &mut Vec<Instruction<'static>>) {
    message_error(body, "Invalid nominal operation");
}
pub(super) fn message_error(body: &mut Vec<Instruction<'static>>, text: &str) {
    use Instruction::*;
    let message: Vec<_> = text.encode_utf16().collect();
    body.push(GlobalGet(ERROR_GLOBAL));
    body.extend(message.iter().map(|unit| I32Const(*unit as i32)));
    body.extend([
        ArrayNewFixed {
            array_type_index: STRING,
            array_size: message.len() as u32,
        },
        I32Const(0),
        RefI31,
        I32Const(0),
        RefI31,
        I32Const(0),
        RefI31,
        StructNew(8),
        Throw(0),
    ]);
}
fn guard(body: &mut Vec<Instruction<'static>>, local: u32, ty: u32) {
    use Instruction::*;
    body.extend([
        LocalGet(local),
        RefTestNonNull(HeapType::Concrete(ty)),
        I32Eqz,
        If(BlockType::Empty),
    ]);
    error(body);
    body.push(End);
}
fn get(body: &mut Vec<Instruction<'static>>, local: u32, ty: u32, field: u32) {
    use Instruction::*;
    body.extend([
        LocalGet(local),
        RefCastNonNull(HeapType::Concrete(ty)),
        StructGet {
            struct_type_index: ty,
            field_index: field,
        },
    ]);
}
fn array(body: &mut Vec<Instruction<'static>>, local: u32) {
    body.extend([
        Instruction::LocalGet(local),
        Instruction::RefCastNonNull(HeapType::Concrete(ARGS)),
    ]);
}
fn copy_array(body: &mut Vec<Instruction<'static>>, source: u32, target: u32) {
    use Instruction::*;
    array(body, source);
    body.extend([ArrayLen, ArrayNewDefault(ARGS), LocalSet(target)]);
    array(body, target);
    body.push(I32Const(0));
    array(body, source);
    body.push(I32Const(0));
    array(body, source);
    body.extend([
        ArrayLen,
        ArrayCopy {
            array_type_index_dst: ARGS,
            array_type_index_src: ARGS,
        },
    ]);
}

pub(super) fn functions(b: &mut Builder, generic_invoke: u32) -> Vec<u32> {
    use Instruction::*;
    let mut body = vec![];
    guard(&mut body, 0, ARGS);
    body.extend([
        GlobalGet(ID_GLOBAL),
        I64Const(i64::MAX),
        I64Eq,
        If(BlockType::Empty),
    ]);
    error(&mut body);
    body.push(End);
    copy_array(&mut body, 0, 1);
    body.extend([
        GlobalGet(ID_GLOBAL),
        LocalGet(1),
        I32Const(0),
        ArrayNewDefault(ARGS),
        I32Const(0),
        RefI31,
        I32Const(0),
        RefI31,
        StructNew(DESCRIPTOR),
        GlobalGet(ID_GLOBAL),
        I64Const(1),
        I64Add,
        GlobalSet(ID_GLOBAL),
    ]);
    // The same constructor for core types. Only the stored identity carries the
    // bit (the second counter read; the first is the overflow check), so
    // identities stay unique and the counter advances as usual.
    let mut core = body.clone();
    let identity = core
        .iter()
        .enumerate()
        .filter(|(_, instruction)| matches!(instruction, GlobalGet(ID_GLOBAL)))
        .nth(1)
        .map(|(index, _)| index)
        .expect("descriptor identity read");
    let mut statics = core.clone();
    let mut closed = core.clone();
    core.splice(identity + 1..identity + 1, [I64Const(CORE_PRIVATE_BIT | CORE_CLOSED_BIT), I64Or]);
    statics.splice(identity + 1..identity + 1, [I64Const(CORE_PRIVATE_BIT | CORE_PUBLIC_STATICS_BIT | CORE_CLOSED_BIT), I64Or]);
    closed.splice(identity + 1..identity + 1, [I64Const(CORE_CLOSED_BIT), I64Or]);
    b.function_with_locals("descriptor-new-core-statics", &[VALUE], &[VALUE], &[(1, VALUE)], &statics);
    b.function_with_locals("descriptor-new-core-closed", &[VALUE], &[VALUE], &[(1, VALUE)], &closed);
    b.function_with_locals("descriptor-new", &[VALUE], &[VALUE], &[(1, VALUE)], &body);
    b.function_with_locals("descriptor-new-core", &[VALUE], &[VALUE], &[(1, VALUE)], &core);

    // Unprivileged named access: reject built-in instances and class values.

    let mut body = vec![];
    guard(&mut body, 0, DESCRIPTOR);
    guard(&mut body, 1, ARGS);
    get(&mut body, 0, DESCRIPTOR, 1);
    body.push(LocalSet(2));
    guard(&mut body, 2, ARGS);
    array(&mut body, 2);
    body.extend([ArrayLen, LocalTee(10)]);
    array(&mut body, 1);
    body.extend([ArrayLen, I32Ne, If(BlockType::Empty)]);
    error(&mut body);
    body.push(End);
    // Keep constructor argument evaluation outside this storage normalization.
    // Equal source-munged field names receive the last argument, as in the
    // pinned JS constructor's duplicate parameter/assignment spelling.
    copy_array(&mut body, 1, 3);
    // The schema and STRING lengths cannot change within this callback-free
    // normalization. Read them once, but keep every field/unit load live on
    // each object construction; descriptor schemas are externally mutable.
    body.extend([I32Const(0), LocalSet(4), Block(BlockType::Empty), Loop(BlockType::Empty),
        LocalGet(10), LocalGet(4), I32LeU, BrIf(1)]);
    array(&mut body, 2);
    body.extend([LocalGet(4), ArrayGet(ARGS), LocalSet(6),
        LocalGet(6), RefTestNonNull(HeapType::Concrete(STRING)), If(BlockType::Empty),
        LocalGet(6), RefCastNonNull(HeapType::Concrete(STRING)), ArrayLen, LocalSet(11),
        LocalGet(4), I32Const(1), I32Add, LocalSet(5),
        Block(BlockType::Empty), Loop(BlockType::Empty),
        LocalGet(10), LocalGet(5), I32LeU, BrIf(1)]);
    array(&mut body, 2);
    body.extend([LocalGet(5), ArrayGet(ARGS), LocalSet(7),
        LocalGet(7), RefTestNonNull(HeapType::Concrete(STRING)), If(BlockType::Empty),
        LocalGet(11),
        LocalGet(7), RefCastNonNull(HeapType::Concrete(STRING)), ArrayLen, I32Eq,
        LocalSet(9), I32Const(0), LocalSet(8),
        Block(BlockType::Empty), Loop(BlockType::Empty),
        LocalGet(9), I32Eqz, BrIf(1),
        LocalGet(8), LocalGet(11), I32GeU, BrIf(1),
        LocalGet(6), RefCastNonNull(HeapType::Concrete(STRING)), LocalGet(8), ArrayGetU(STRING),
        LocalGet(7), RefCastNonNull(HeapType::Concrete(STRING)), LocalGet(8), ArrayGetU(STRING),
        I32Eq, LocalSet(9), LocalGet(8), I32Const(1), I32Add, LocalSet(8), Br(0), End, End,
        LocalGet(9), If(BlockType::Empty)]);
    array(&mut body, 3);
    body.push(LocalGet(4));
    array(&mut body, 1);
    body.extend([LocalGet(5), ArrayGet(ARGS), ArraySet(ARGS), End, End,
        LocalGet(5), I32Const(1), I32Add, LocalSet(5), Br(0), End, End, End,
        LocalGet(4), I32Const(1), I32Add, LocalSet(4), Br(0), End, End]);
    body.extend([
        LocalGet(0),
        RefCastNonNull(HeapType::Concrete(DESCRIPTOR)),
        LocalGet(3),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        LocalGet(0),
        Call(b.names["owned-descriptor-prototype"]),
        ArrayNewFixed {
            array_type_index: ARGS,
            array_size: 1,
        },
        I32Const(0),
        RefI31,
        StructNew(OBJECT),
    ]);
    let object_new = b.function_with_locals(
        "object-new",
        &[VALUE, VALUE],
        &[VALUE],
        &[(2, VALUE), (2, ValType::I32), (2, VALUE), (4, ValType::I32)],
        &body,
    );

    let mut body = vec![];
    guard(&mut body, 0, DESCRIPTOR);
    body.extend([
        LocalGet(0),
        GlobalGet(ERROR_GLOBAL),
        RefEq,
        If(BlockType::Empty),
    ]);
    super::error_constructors::family_test(b, &mut body, 1);
    body.extend([Return, End]);
    body.extend([
        LocalGet(0),
        Call(b.names["owned-descriptor-prototype"]),
        LocalSet(2),
        LocalGet(2),
        I32Const(0),
        RefI31,
        RefEq,
        I32Eqz,
        If(BlockType::Empty),
        LocalGet(1),
        LocalGet(2),
        Call(b.names["owned-prototype-instance"]),
        Return,
        End,
    ]);
    body.extend([
        LocalGet(1),
        RefTestNonNull(HeapType::Concrete(OBJECT)),
        If(BlockType::Result(ValType::I32)),
    ]);
    get(&mut body, 1, OBJECT, 0);
    body.extend([
        LocalGet(0),
        RefEq,
        Else,
        LocalGet(1),
        RefTestNonNull(HeapType::Concrete(8)),
        If(BlockType::Result(ValType::I32)),
    ]);
    get(&mut body, 1, 8, 0);
    body.extend([LocalGet(0), RefEq, Else, I32Const(0), End, End]);
    b.function_with_locals(
        "object-instance",
        &[VALUE, VALUE],
        &[ValType::I32],
        &[(1, VALUE)],
        &body,
    );

    let mut body = vec![];
    guard(&mut body, 0, OBJECT);
    get(&mut body, 0, OBJECT, 0);
    let object_descriptor = b.function("object-descriptor", &[VALUE], &[VALUE], &body);

    // Genuine nominal identity: compare the immutable, rooted descriptors,
    // never a type spelling, numeric identity, structural layout or mutable
    // constructor property. Other value families cannot share this descriptor.
    let mut body = vec![
        LocalGet(0), RefTestNonNull(HeapType::Concrete(OBJECT)),
        LocalGet(1), RefTestNonNull(HeapType::Concrete(OBJECT)),
        I32And, If(BlockType::Result(ValType::I32)),
    ];
    get(&mut body, 0, OBJECT, 0);
    get(&mut body, 1, OBJECT, 0);
    body.extend([RefEq, Else, I32Const(0), End]);
    b.function("object-same-descriptor", &[VALUE, VALUE], &[ValType::I32], &body);

    for writing in [false, true] {
        let mut body = vec![];
        guard(&mut body, 0, OBJECT);
        get(&mut body, 0, OBJECT, 1);
        body.extend([LocalSet(if writing { 3 } else { 2 }), LocalGet(1)]);
        array(&mut body, if writing { 3 } else { 2 });
        body.extend([ArrayLen, I32GeU, If(BlockType::Empty)]);
        error(&mut body);
        body.push(End);
        array(&mut body, if writing { 3 } else { 2 });
        body.push(LocalGet(1));
        if writing {
            body.extend([LocalGet(2), ArraySet(ARGS)]);
        } else {
            body.push(ArrayGet(ARGS));
        }
        b.function_with_locals(
            if writing {
                "object-field-set"
            } else {
                "object-field-get"
            },
            if writing {
                &[VALUE, ValType::I32, VALUE]
            } else {
                &[VALUE, ValType::I32]
            },
            if writing { &[] } else { &[VALUE] },
            &[(1, VALUE)],
            &body,
        );
    }

    // Each method key is a rooted nominal descriptor. Matching by reference avoids
    // structural identity or forged duplicate numeric identities.
    let mut method_get = 0;
    let mut method_set = 0;
    for writing in [false, true] {
        let index = if writing { 3 } else { 2 };
        let table = index + 1;
        let fresh = index + 2;
        let length = table + if writing { 2 } else { 1 };
        let typed_table = length + 1;
        let mut body = vec![];
        guard(&mut body, 0, DESCRIPTOR);
        guard(&mut body, 1, DESCRIPTOR);
        if writing {
            body.extend([
                LocalGet(2),
                RefTestNonNull(HeapType::Concrete(4)),
                I32Eqz,
                If(BlockType::Empty),
                LocalGet(2),
                GlobalGet(SENTINEL_GLOBAL),
                RefEq,
                I32Eqz,
                If(BlockType::Empty),
            ]);
            error(&mut body);
            body.push(End);
            get(&mut body, 1, DESCRIPTOR, 1);
            body.push(LocalSet(fresh));
            guard(&mut body, fresh, ARGS);
            array(&mut body, fresh);
            body.extend([ArrayLen, If(BlockType::Empty)]);
            error(&mut body);
            body.extend([End, End]);
        }
        get(&mut body, 0, DESCRIPTOR, 2);
        body.push(LocalSet(table));
        guard(&mut body, table, ARGS);
        // Retain the checked array in a typed local for this call only. No
        // callback occurs during lookup; later calls still load the live table.
        body.extend([LocalGet(table), RefCastNonNull(HeapType::Concrete(ARGS)), LocalSet(typed_table)]);
        // Reject malformed odd-length tables before pair indexing.
        body.push(LocalGet(typed_table));
        body.extend([ArrayLen, LocalTee(length), I32Const(1), I32And, If(BlockType::Empty)]);
        error(&mut body);
        body.push(End);
        if !writing {
            fn pair(body: &mut Vec<Instruction<'static>>, table: u32, key: u32, offset: i32) {
                body.extend([LocalGet(table), I32Const(offset), ArrayGet(ARGS),
                    LocalGet(key), RefEq, If(BlockType::Empty), LocalGet(table),
                    I32Const(offset + 1), ArrayGet(ARGS), Return, End]);
            }
            fn exhausted(body: &mut Vec<Instruction<'static>>, length: u32, offset: i32) {
                body.extend([LocalGet(length), I32Const(offset), I32LeU, If(BlockType::Empty),
                    I32Const(0), RefI31, Return, End]);
            }
            // Preserve the original short-table/early-hit path exactly. Later
            // complete groups share bounds checks; partial groups still check
            // each remaining pair in order, without a variable-index loop.
            for offset in (0..8).step_by(2) {
                exhausted(&mut body, length, offset);
                pair(&mut body, typed_table, 1, offset);
            }
            for group in (8..32).step_by(8) {
                exhausted(&mut body, length, group);
                body.extend([LocalGet(length), I32Const(group + 8), I32LtU, If(BlockType::Empty)]);
                for offset in (group..group + 8).step_by(2) {
                    if offset != group { exhausted(&mut body, length, offset); }
                    pair(&mut body, typed_table, 1, offset);
                }
                body.extend([I32Const(0), RefI31, Return, Else]);
                for offset in (group..group + 8).step_by(2) {
                    pair(&mut body, typed_table, 1, offset);
                }
                body.push(End);
            }
        }
        body.extend([
            I32Const(if writing { 0 } else { 32 }),
            LocalSet(index),
            Block(BlockType::Empty),
            Loop(BlockType::Empty),
            LocalGet(index),
        ]);
        // No callback occurs during this table walk. Its GC array length is
        // immutable even when an existing slot is replaced, so read it once.
        body.extend([LocalGet(length), I32GeU, BrIf(1)]);
        body.push(LocalGet(typed_table));
        body.extend([
            LocalGet(index),
            ArrayGet(ARGS),
            LocalGet(1),
            RefEq,
            If(BlockType::Empty),
        ]);
        body.push(LocalGet(typed_table));
        body.extend([LocalGet(index), I32Const(1), I32Add]);
        if writing {
            body.extend([LocalGet(2), ArraySet(ARGS), Return]);
        } else {
            body.extend([ArrayGet(ARGS), Return]);
        }
        body.extend([
            End,
            LocalGet(index),
            I32Const(2),
            I32Add,
            LocalSet(index),
            Br(0),
            End,
            End,
        ]);
        if writing {
            body.extend([
                LocalGet(length),
                I32Const(i32::MAX - 2),
                I32GtU,
                If(BlockType::Empty),
            ]);
            error(&mut body);
            body.push(End);
            body.extend([
                LocalGet(length),
                I32Const(2),
                I32Add,
                ArrayNewDefault(ARGS),
                LocalSet(fresh),
            ]);
            array(&mut body, fresh);
            body.push(I32Const(0));
            body.push(LocalGet(typed_table));
            body.push(I32Const(0));
            body.extend([
                LocalGet(length),
                ArrayCopy {
                    array_type_index_dst: ARGS,
                    array_type_index_src: ARGS,
                },
            ]);
            for (offset, value) in [(0, 1), (1, 2)] {
                array(&mut body, fresh);
                body.push(LocalGet(index));
                if offset != 0 {
                    body.extend([I32Const(offset), I32Add]);
                }
                body.extend([LocalGet(value), ArraySet(ARGS)]);
            }
            body.extend([
                LocalGet(0),
                RefCastNonNull(HeapType::Concrete(DESCRIPTOR)),
                LocalGet(fresh),
                StructSet {
                    struct_type_index: DESCRIPTOR,
                    field_index: 2,
                },
            ]);
        } else {
            body.extend([I32Const(0), RefI31]);
        }
        let method_function = b.function_with_locals(
            if writing {
                "protocol-method-set"
            } else {
                "protocol-method-get"
            },
            if writing {
                &[VALUE, VALUE, VALUE]
            } else {
                &[VALUE, VALUE]
            },
            if writing { &[] } else { &[VALUE] },
            &[(1, ValType::I32), (if writing { 2 } else { 1 }, VALUE), (1, ValType::I32), (1, reference(ARGS))],
            &body,
        );
        if !writing {
            method_get = method_function;
        } else {
            method_set = method_function;
        }
    }
    let constructor = invoke_body(b, &[LocalGet(0), LocalGet(1), Call(object_new)]);
    // Dispatch always consults the live table, including for previously created
    // instances and previously captured dispatchers.
    let mut dispatch_body = vec![
        LocalGet(1),
        I32Const(0),
        ArrayGet(ARGS),
        Call(object_descriptor),
        LocalGet(0),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        I32Const(0),
        ArrayGet(ARGS),
        Call(method_get),
        LocalSet(2),
    ];
    guard(&mut dispatch_body, 2, 4);
    dispatch_body.extend([LocalGet(2), LocalGet(1), Call(generic_invoke)]);
    let dispatcher = invoke_body_with_locals(b, &[(1, VALUE)], &dispatch_body);
    for (name, callback, minimum) in [
        ("constructor-new", constructor, 0),
        ("protocol-dispatcher-new", dispatcher, 1),
    ] {
        let mut body = vec![];
        guard(&mut body, 0, DESCRIPTOR);
        get(&mut body, 0, DESCRIPTOR, 1);
        body.push(LocalSet(2));
        guard(&mut body, 2, ARGS);
        array(&mut body, 2);
        body.extend([
            ArrayLen,
            LocalSet(1),
            LocalGet(1),
            I32Const(minimum),
            I32LtU,
            If(BlockType::Empty),
        ]);
        error(&mut body);
        body.push(End);
        body.push(LocalGet(0));
        if minimum == 1 {
            // A protocol dispatcher is not a constructor. Keep its environment
            // distinct so constructor-descriptor rejects it before field use.
            body.push(ArrayNewFixed {
                array_type_index: ARGS,
                array_size: 1,
            });
        }
        body.extend([
            RefFunc(callback),
            LocalGet(1),
            LocalGet(1),
            Call(b.names["closure-new"]),
        ]);
        b.function_with_locals(
            name,
            &[VALUE],
            &[VALUE],
            &[(1, ValType::I32), (1, VALUE)],
            &body,
        );
    }
    let mut body = vec![];
    guard(&mut body, 0, 4);
    body.extend([LocalGet(0), Call(b.names["closure-environment"])]);
    body.push(LocalSet(1));
    guard(&mut body, 1, DESCRIPTOR);
    body.push(LocalGet(1));
    b.function_with_locals(
        "constructor-descriptor",
        &[VALUE],
        &[VALUE],
        &[(1, VALUE)],
        &body,
    );
    // Source type values are callable, but ordinary calls do not construct an
    // instance. Dotted/new lowering uses constructor-new separately.
    let ordinary_type = invoke_body(b, &[I32Const(UNDEFINED), RefI31]);
    let mut body = vec![];
    guard(&mut body, 0, DESCRIPTOR);
    body.extend([
        LocalGet(0),
        GlobalGet(type_values::KEY_GLOBAL),
        Call(method_get),
        LocalSet(1),
        LocalGet(1),
        RefTestNonNull(HeapType::Concrete(4)),
        If(BlockType::Empty),
        LocalGet(1),
        Return,
        End,
        LocalGet(0),
        RefFunc(ordinary_type),
        I32Const(0),
        I32Const(-1),
        Call(b.names["closure-new"]),
        LocalSet(1),
        LocalGet(0),
        GlobalGet(type_values::KEY_GLOBAL),
        LocalGet(1),
        Call(method_set),
        LocalGet(1),
    ]);
    b.function_with_locals("class-value-new", &[VALUE], &[VALUE], &[(1, VALUE)], &body);
    // A source type value is a closure owning its descriptor. Ordinary functions
    // and protocol values also use closures, but own arrays instead. Unwrap the
    // property storage before inspecting that environment; never infer a type
    // value from the outer closure layout or a name registered by the host.
    b.function(
        "class-value-is",
        &[VALUE],
        &[ValType::I32],
        &[
            LocalGet(0),
            RefTestNonNull(HeapType::Concrete(4)),
            If(BlockType::Result(ValType::I32)),
            LocalGet(0),
            Call(b.names["closure-environment"]),
            RefTestNonNull(HeapType::Concrete(DESCRIPTOR)),
            Else,
            I32Const(0),
            End,
        ],
    );
    let mut body = vec![];
    guard(&mut body, 0, ARGS);
    body.extend([
        LocalGet(0),
        RefFunc(ordinary_type),
        I32Const(0),
        I32Const(-1),
        Call(b.names["closure-new"]),
    ]);
    b.function("protocol-value-new", &[VALUE], &[VALUE], &body);
    let mut body = vec![];
    guard(&mut body, 0, 4);
    body.extend([LocalGet(0), Call(b.names["closure-environment"])]);
    body.push(LocalSet(2));
    guard(&mut body, 2, ARGS);
    body.extend([LocalGet(1)]);
    array(&mut body, 2);
    body.extend([ArrayLen, I32GeU, If(BlockType::Empty)]);
    error(&mut body);
    body.push(End);
    array(&mut body, 2);
    body.extend([LocalGet(1), ArrayGet(ARGS), LocalSet(2)]);
    guard(&mut body, 2, DESCRIPTOR);
    body.push(LocalGet(2));
    let protocol_key = b.function_with_locals(
        "protocol-key",
        &[VALUE, ValType::I32],
        &[VALUE],
        &[(1, VALUE)],
        &body,
    );
    let mut body = vec![];
    body.extend([
        LocalGet(0),
        I32Const(0),
        Call(protocol_key),
        LocalSet(2),
        LocalGet(1),
        RefTestNonNull(HeapType::Concrete(OBJECT)),
        If(BlockType::Result(ValType::I32)),
    ]);
    get(&mut body, 1, OBJECT, 0);
    body.extend([
        LocalGet(2),
        Call(method_get),
        GlobalGet(SENTINEL_GLOBAL),
        RefEq,
        Else,
        I32Const(0),
        End,
    ]);
    b.function_with_locals(
        "protocol-satisfies",
        &[VALUE, VALUE],
        &[ValType::I32],
        &[(1, VALUE)],
        &body,
    );
    let mut body = vec![];
    guard(&mut body, 0, DESCRIPTOR);
    body.extend([
        LocalGet(1),
        RefTestNonNull(HeapType::Concrete(OBJECT)),
        If(BlockType::Result(ValType::I32)),
    ]);
    get(&mut body, 1, OBJECT, 0);
    body.extend([
        LocalGet(0),
        Call(method_get),
        GlobalGet(SENTINEL_GLOBAL),
        RefEq,
        Else,
        I32Const(0),
        End,
    ]);
    b.function(
        "protocol-marker-satisfies",
        &[VALUE, VALUE],
        &[ValType::I32],
        &body,
    );
    // The source constructor follows the pin's JS constructor convention: all
    // arguments evaluate, extra fields are ignored, missing fields become undefined.
    let mut body = vec![];
    get(&mut body, 0, DESCRIPTOR, 1);
    body.extend([
        RefCastNonNull(HeapType::Concrete(ARGS)),
        ArrayLen,
        LocalSet(2),
        LocalGet(1),
        ArrayLen,
        LocalSet(3),
        I32Const(UNDEFINED),
        RefI31,
        LocalGet(2),
        ArrayNew(ARGS),
        LocalSet(4),
    ]);
    array(&mut body, 4);
    body.extend([
        I32Const(0),
        LocalGet(1),
        I32Const(0),
        LocalGet(2),
        LocalGet(3),
        I32LtU,
        If(BlockType::Result(ValType::I32)),
        LocalGet(2),
        Else,
        LocalGet(3),
        End,
        ArrayCopy {
            array_type_index_dst: ARGS,
            array_type_index_src: ARGS,
        },
        LocalGet(0),
        LocalGet(4),
        Call(object_new),
    ]);
    b.function(
        "protocol-marker-set",
        &[VALUE, VALUE],
        &[VALUE],
        &[
            LocalGet(0),
            LocalGet(1),
            GlobalGet(SENTINEL_GLOBAL),
            Call(method_set),
            GlobalGet(SENTINEL_GLOBAL),
        ],
    );
    // The compiler bridge may use a validated descriptor plan instead of
    // allocating a closure. Share the exact padding/copying/normalization body.
    b.function_with_locals(
        "nominal-source-object-new", &[VALUE, reference(ARGS)], &[VALUE],
        &[(2, ValType::I32), (1, VALUE)], &body,
    );
    let source_constructor = invoke_body_with_locals(b, &[(2, ValType::I32), (1, VALUE)], &body);
    let mut body = vec![];
    guard(&mut body, 0, DESCRIPTOR);
    get(&mut body, 0, DESCRIPTOR, 1);
    body.push(LocalSet(1));
    guard(&mut body, 1, ARGS);
    let mut plan = body.clone();
    plan.push(LocalGet(0));
    b.function_with_locals(
        "nominal-source-constructor-plan", &[VALUE], &[VALUE], &[(1, VALUE)], &plan,
    );
    body.extend([
        LocalGet(0),
        RefFunc(source_constructor),
        I32Const(0),
        I32Const(-1),
        Call(b.names["closure-new"]),
    ]);
    b.function_with_locals(
        "nominal-source-constructor-new",
        &[VALUE],
        &[VALUE],
        &[(1, VALUE)],
        &body,
    );
    let native_functions =
        native_protocols::functions(b, generic_invoke, method_get, object_descriptor);
    let mut functions = vec![constructor, dispatcher, ordinary_type, source_constructor];
    functions.extend(native_functions);
    functions
}

fn invoke_body(b: &mut Builder, instructions: &[Instruction<'_>]) -> u32 {
    invoke_body_with_locals(b, &[], instructions)
}
fn invoke_body_with_locals(
    b: &mut Builder,
    locals: &[(u32, ValType)],
    instructions: &[Instruction<'_>],
) -> u32 {
    let index = b.count;
    b.functions.function(INVOKE);
    let mut body = Function::new(locals.iter().copied());
    for instruction in instructions {
        body.instruction(instruction);
    }
    body.instruction(&Instruction::End);
    b.code.function(&body);
    b.count += 1;
    index
}
