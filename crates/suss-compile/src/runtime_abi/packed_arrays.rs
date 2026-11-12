//! Packed primitive arrays (ADR-0003, #336).
//!
//! `byte-array`, `short-array`, `int-array`, `float-array`, `long-array` and
//! `double-array` hold their elements in packed WasmGC storage. `aget` returns
//! an element as a long or double; `aset` converts its value with the cast of
//! the element type, so a narrow integer array checks the range, while core's
//! constructors store with the unchecked casts, as Java's `Number` methods do.
use super::*;

/// Element types, in `PACKED_STORAGE` order: name, checked cast and unchecked
/// cast, as `bitwise::CASTS` names them, and the JVM's array class name.
pub(crate) const KINDS: [(&str, &str, &str, &str); 6] = [
    ("byte", "byte", "unchecked-byte", "[B"),
    ("short", "short", "unchecked-short", "[S"),
    ("int", "int", "unchecked-int", "[I"),
    // aset rounds a float, as Clojure's float conversion does.
    ("float", "unchecked-float", "unchecked-float", "[F"),
    ("long", "long", "unchecked-long", "[J"),
    ("double", "double", "double", "[D"),
];

pub(super) fn storage(kind: usize) -> u32 {
    PACKED_STORAGE + kind as u32
}
pub(super) fn string(text: &str) -> Vec<Instruction<'static>> {
    let units = text.encode_utf16().collect::<Vec<_>>();
    let mut code = units
        .iter()
        .map(|unit| Instruction::I32Const(i32::from(*unit)))
        .collect::<Vec<_>>();
    code.push(Instruction::ArrayNewFixed {
        array_type_index: STRING,
        array_size: units.len() as u32,
    });
    code
}
/// Throw a language error with the message on the stack.
fn throw() -> [Instruction<'static>; 6] {
    use Instruction::*;
    [
        RefCastNonNull(HeapType::Concrete(STRING)),
        sentinel_value(NIL),
        sentinel_value(NIL),
        I32Const(0), // Identity UID, unassigned.
        RefI31,
        StructNew(8),
    ]
}
fn raise(b: &Builder, parts: &[Result<&str, u32>]) -> Vec<Instruction<'static>> {
    use Instruction::*;
    // Each part is literal text or a local holding a value to print.
    let mut code = vec![GlobalGet(nominal::ERROR_GLOBAL)];
    for (index, part) in parts.iter().enumerate() {
        match part {
            Ok(text) => code.extend(string(text)),
            Err(local) => code.extend([LocalGet(*local), Call(b.names["coerce-string"])]),
        }
        if index > 0 {
            code.push(Call(b.names["string-concat"]));
        }
    }
    code.extend(throw());
    code.push(Throw(0));
    code
}
/// The storage of the packed array in local 0, into local `data`.
fn data(data: u32) -> [Instruction<'static>; 4] {
    use Instruction::*;
    [
        LocalGet(0),
        RefCastNonNull(HeapType::Concrete(PACKED)),
        StructGet {
            struct_type_index: PACKED,
            field_index: 0,
        },
        LocalSet(data),
    ]
}
/// For each element type, `body(kind)` when local `data` holds its storage;
/// each body returns.
fn each_kind(
    code: &mut Vec<Instruction<'static>>,
    data: u32,
    mut body: impl FnMut(usize) -> Vec<Instruction<'static>>,
) {
    use Instruction::*;
    for kind in 0..KINDS.len() {
        code.extend([
            LocalGet(data),
            RefTestNonNull(HeapType::Concrete(storage(kind))),
            If(BlockType::Empty),
        ]);
        code.extend(body(kind));
        code.push(End);
    }
    code.push(Unreachable);
}
fn cast_storage(kind: usize, data: u32) -> [Instruction<'static>; 2] {
    [
        Instruction::LocalGet(data),
        Instruction::RefCastNonNull(HeapType::Concrete(storage(kind))),
    ]
}
/// The element at the index on the stack of the storage in `data`, as a value.
fn element(b: &Builder, kind: usize) -> Vec<Instruction<'static>> {
    use Instruction::*;
    let ty = storage(kind);
    match kind {
        0 | 1 => vec![ArrayGetS(ty), I64ExtendI32S, Call(b.names["long-box"])],
        2 => vec![ArrayGet(ty), I64ExtendI32S, Call(b.names["long-box"])],
        3 => vec![ArrayGet(ty), F64PromoteF32, Call(b.names["number-box"])],
        4 => vec![ArrayGet(ty), Call(b.names["long-box"])],
        _ => vec![ArrayGet(ty), Call(b.names["number-box"])],
    }
}
/// Unbox a converted value on the stack to its storage's element operand.
fn operand(b: &Builder, kind: usize) -> Vec<Instruction<'static>> {
    use Instruction::*;
    match kind {
        0..=2 => vec![Call(b.names["long-unbox"]), I32WrapI64],
        // Already rounded to binary32, so the demotion is exact.
        3 => vec![Call(b.names["number-unbox"]), F32DemoteF64],
        4 => vec![Call(b.names["long-unbox"])],
        _ => vec![Call(b.names["number-unbox"])],
    }
}
/// The index value in local 1 as an i32 in local `index`, checked against the
/// length of the storage in `data` (local `length`), as Java's arrays check it.
fn index(b: &Builder, kind: usize, data: u32, index: u32, value: u32, length: u32) -> Vec<Instruction<'static>> {
    use Instruction::*;
    let mut code = vec![
        // aget's index is `(int i)`, as Clojure's inline aget casts it.
        LocalGet(1),
        Call(b.names["number-cast-int"]),
        LocalTee(value),
        Call(b.names["long-unbox"]),
        I32WrapI64,
        LocalSet(index),
    ];
    code.extend(cast_storage(kind, data));
    code.extend([
        ArrayLen,
        LocalSet(length),
        LocalGet(index),
        LocalGet(length),
        I32GeU,
        If(BlockType::Empty),
        LocalGet(length),
        I64ExtendI32U,
        Call(b.names["long-box"]),
        LocalSet(data),
    ]);
    code.extend(raise(
        b,
        &[Ok("Index "), Err(value), Ok(" out of bounds for length "), Err(data)],
    ));
    code.push(End);
    code
}

pub(super) fn functions(b: &mut Builder) {
    use Instruction::*;
    b.function(
        "packed-array?",
        &[VALUE],
        &[ValType::I32],
        &[LocalGet(0), RefTestNonNull(HeapType::Concrete(PACKED))],
    );
    // The same test as a boolean, for core's `suss.bootstrap/packed-array?`.
    let mut body = vec![LocalGet(0), RefTestNonNull(HeapType::Concrete(PACKED))];
    body.extend(boolean());
    b.function("packed-array-test", &[VALUE], &[VALUE], &body);
    // A new zeroed array: element kind and length, as core's constructors
    // pass them. The length is `(int n)`, so 2.5 makes two elements.
    let mut body = vec![
        LocalGet(1),
        Call(b.names["number-cast-int"]),
        LocalTee(1),
        Call(b.names["long-unbox"]),
        I32WrapI64,
        LocalTee(2),
        I32Const(0),
        I32LtS,
        If(BlockType::Empty),
    ];
    // Java's NegativeArraySizeException message is the length alone.
    body.extend(raise(b, &[Ok(""), Err(1)]));
    body.extend([End, LocalGet(2), I32Const(arrays::MAX_LENGTH), I32GtS, If(BlockType::Empty)]);
    body.extend(raise(b, &[Ok("Array length out of range: "), Err(1)]));
    body.extend([End, LocalGet(0), Call(b.names["long-unbox"]), I32WrapI64, LocalSet(3)]);
    for kind in 0..KINDS.len() {
        body.extend([
            LocalGet(3),
            I32Const(kind as i32),
            I32Eq,
            If(BlockType::Empty),
            LocalGet(2),
            ArrayNewDefault(storage(kind)),
            I32Const(0), // Identity UID, unassigned.
            RefI31,
            StructNew(PACKED),
            Return,
            End,
        ]);
    }
    body.extend(raise(b, &[Ok("Invalid packed array kind: "), Err(0)]));
    b.function_with_locals(
        "packed-array-new",
        &[VALUE, VALUE],
        &[VALUE],
        &[(2, ValType::I32)],
        &body,
    );
    let mut body = data(1).to_vec();
    each_kind(&mut body, 1, |kind| {
        let mut code = cast_storage(kind, 1).to_vec();
        code.extend([ArrayLen, Call(b.names["long-box-u32"]), Return]);
        code
    });
    b.function_with_locals("packed-array-length", &[VALUE], &[VALUE], &[(1, VALUE)], &body);
    // Locals: 2 storage, 3 index, 4 index value, 5 length.
    let mut body = data(2).to_vec();
    each_kind(&mut body, 2, |kind| {
        let mut code = index(b, kind, 2, 3, 4, 5);
        code.extend(cast_storage(kind, 2));
        code.push(LocalGet(3));
        code.extend(element(b, kind));
        code.push(Return);
        code
    });
    b.function_with_locals(
        "packed-array-get",
        &[VALUE, VALUE],
        &[VALUE],
        &[(1, VALUE), (1, ValType::I32), (1, VALUE), (1, ValType::I32)],
        &body,
    );
    // aset converts with the checked cast, and core's constructors with the
    // unchecked one; both return the stored value. Locals as above.
    for (name, checked) in [("packed-array-set", true), ("packed-array-store", false)] {
        let mut body = data(3).to_vec();
        each_kind(&mut body, 3, |kind| {
            let (_, checked_cast, unchecked_cast, _) = KINDS[kind];
            let cast = if checked { checked_cast } else { unchecked_cast };
            let mut code = index(b, kind, 3, 4, 5, 6);
            code.extend([LocalGet(2), Call(b.names[&format!("number-cast-{cast}")]), LocalSet(2)]);
            code.extend(cast_storage(kind, 3));
            code.extend([LocalGet(4), LocalGet(2)]);
            code.extend(operand(b, kind));
            code.extend([ArraySet(storage(kind)), LocalGet(2), Return]);
            code
        });
        b.function_with_locals(
            name,
            &[VALUE, VALUE, VALUE],
            &[VALUE],
            &[(1, VALUE), (1, ValType::I32), (1, VALUE), (1, ValType::I32)],
            &body,
        );
    }
    // `(aget a i)` and `(aset a i v)` on a packed array (#389): an i31 index
    // inside the length reads or writes the storage directly. aset does so for
    // the values whose conversion is exact: an i31 into long or int storage,
    // and an i31 or boxed double into double storage. Anything else, and every
    // error, takes packed-array-get or packed-array-set.
    // Kinds in order of likely use, each cast once (br_on_cast_fail) into its
    // own typed local. Locals: 2 storage, 3 index, then one per kind.
    const ORDER: [usize; 6] = [4, 5, 2, 0, 1, 3];
    let typed = |base: u32, kind: usize| base + kind as u32;
    let locals = |base: usize| {
        let mut locals = vec![(1, VALUE), (1, ValType::I32)];
        locals.extend((0..KINDS.len()).map(|kind| (1, ValType::Ref(RefType { nullable: true, heap_type: HeapType::Concrete(storage(kind)) }))));
        let _ = base;
        locals
    };
    let cast_to = |body: &mut Vec<Instruction<'static>>, data: u32, local: u32, kind: usize| {
        body.extend([
            Block(BlockType::Result(VALUE)),
            LocalGet(data),
            BrOnCastFail {
                relative_depth: 0,
                from_ref_type: RefType::EQREF,
                to_ref_type: RefType { nullable: false, heap_type: HeapType::Concrete(storage(kind)) },
            },
            LocalSet(local),
        ]);
    };
    let mut body = vec![Block(BlockType::Empty)];
    body.extend([LocalGet(1), RefTestNonNull(HeapType::I31), I32Eqz, BrIf(0)]);
    body.extend([LocalGet(0), RefTestNonNull(HeapType::Concrete(PACKED)), I32Eqz, BrIf(0)]);
    body.extend(data(2));
    body.extend([LocalGet(1), RefCastNonNull(HeapType::I31), I31GetS, LocalSet(3)]);
    for kind in ORDER {
        let local = typed(4, kind);
        cast_to(&mut body, 2, local, kind);
        // Inside the cast's block, the slow path is one out.
        body.extend([LocalGet(3), LocalGet(local), ArrayLen, I32GeU, BrIf(1), LocalGet(local), LocalGet(3)]);
        body.extend(element(b, kind));
        body.extend([Return, End, Drop]);
    }
    body.extend([End, LocalGet(0), LocalGet(1), Call(b.names["packed-array-get"])]);
    b.function_with_locals("packed-array-get-2", &[VALUE, VALUE], &[VALUE], &locals(4), &body);
    // Locals: 3 storage, 4 index, then one per kind.
    let mut body = vec![Block(BlockType::Empty)];
    body.extend([LocalGet(1), RefTestNonNull(HeapType::I31), I32Eqz, BrIf(0)]);
    body.extend([LocalGet(0), RefTestNonNull(HeapType::Concrete(PACKED)), I32Eqz, BrIf(0)]);
    body.extend(data(3));
    body.extend([LocalGet(1), RefCastNonNull(HeapType::I31), I31GetS, LocalSet(4)]);
    // long (4) and int (2) storage take an i31 value as it is; double (5) an
    // i31, converted exactly, or a boxed double as it is. The result is the
    // stored value, as aset's casts give. Other kinds and values fall through.
    for kind in [4, 5, 2] {
        let local = typed(5, kind);
        cast_to(&mut body, 3, local, kind);
        body.extend([LocalGet(4), LocalGet(local), ArrayLen, I32GeU, BrIf(1)]);
        if kind == 5 {
            body.extend([LocalGet(2), RefTestNonNull(HeapType::I31), If(BlockType::Empty)]);
            body.extend([LocalGet(local), LocalGet(4), LocalGet(2), RefCastNonNull(HeapType::I31), I31GetS, F64ConvertI32S, ArraySet(storage(5))]);
            body.extend([LocalGet(local), LocalGet(4), ArrayGet(storage(5)), Call(b.names["number-box"]), Return, End]);
            body.extend([LocalGet(2), RefTestNonNull(HeapType::Concrete(NUMBER)), I32Eqz, BrIf(1)]);
            body.extend([LocalGet(local), LocalGet(4), LocalGet(2), Call(b.names["number-unbox"]), ArraySet(storage(5)), LocalGet(2), Return]);
        } else {
            body.extend([LocalGet(2), RefTestNonNull(HeapType::I31), I32Eqz, BrIf(1)]);
            body.extend([LocalGet(local), LocalGet(4), LocalGet(2), RefCastNonNull(HeapType::I31), I31GetS]);
            if kind == 4 {
                body.push(I64ExtendI32S);
            }
            body.extend([ArraySet(storage(kind)), LocalGet(2), Return]);
        }
        body.extend([End, Drop]);
    }
    body.extend([End, LocalGet(0), LocalGet(1), LocalGet(2), Call(b.names["packed-array-set"])]);
    let mut set_locals = vec![(1, VALUE), (1, ValType::I32)];
    set_locals.extend((0..KINDS.len()).map(|kind| (1, ValType::Ref(RefType { nullable: true, heap_type: HeapType::Concrete(storage(kind)) }))));
    b.function_with_locals("packed-array-set-3", &[VALUE, VALUE, VALUE], &[VALUE], &set_locals, &body);
    // Locals: 1 source storage, 2 length, 3 copy.
    let mut body = data(1).to_vec();
    each_kind(&mut body, 1, |kind| {
        let ty = storage(kind);
        let mut code = cast_storage(kind, 1).to_vec();
        code.extend([ArrayLen, LocalSet(2), LocalGet(2), ArrayNewDefault(ty), LocalSet(3)]);
        code.extend(cast_storage(kind, 3));
        code.push(I32Const(0));
        code.extend(cast_storage(kind, 1));
        code.extend([
            I32Const(0),
            LocalGet(2),
            ArrayCopy {
                array_type_index_dst: ty,
                array_type_index_src: ty,
            },
            LocalGet(3),
            I32Const(0),
            RefI31,
            StructNew(PACKED),
            Return,
        ]);
        code
    });
    b.function_with_locals(
        "packed-array-clone",
        &[VALUE],
        &[VALUE],
        &[(1, VALUE), (1, ValType::I32), (1, VALUE)],
        &body,
    );
    // Core's private `System/arraycopy` of two arrays of one element kind:
    // source, source index, destination, destination index and length, as
    // longs. It returns the destination. Callers are core's own collections,
    // which pass in-bounds ranges; `array.copy` traps otherwise. Locals: 5
    // source storage, 6 destination storage.
    let unbox = |local| [LocalGet(local), Call(b.names["long-unbox"]), I32WrapI64];
    let mut body = data(5).to_vec();
    body.extend([
        LocalGet(2),
        RefCastNonNull(HeapType::Concrete(PACKED)),
        StructGet {
            struct_type_index: PACKED,
            field_index: 0,
        },
        LocalSet(6),
    ]);
    each_kind(&mut body, 5, |kind| {
        let ty = storage(kind);
        let mut code = cast_storage(kind, 6).to_vec();
        code.extend(unbox(3));
        code.extend(cast_storage(kind, 5));
        code.extend(unbox(1));
        code.extend(unbox(4));
        code.extend([
            ArrayCopy {
                array_type_index_dst: ty,
                array_type_index_src: ty,
            },
            LocalGet(2),
            Return,
        ]);
        code
    });
    b.function_with_locals(
        "packed-array-copy",
        &[VALUE; 5],
        &[VALUE],
        &[(2, VALUE)],
        &body,
    );
}
