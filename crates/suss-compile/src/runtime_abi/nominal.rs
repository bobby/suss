//! Original checked nominal storage primitives for the shared ABI.
//! Internal arrays are schemas/fields/tables, not language persistent collections.
use super::*;

pub(super) const ID_GLOBAL: u32 = numeric::ERROR_GLOBALS + 1;
pub(super) const ERROR_GLOBAL: u32 = ID_GLOBAL + 1;
pub(super) const SENTINEL_GLOBAL: u32 = ERROR_GLOBAL + 1;
const OBJECT: u32 = 7;

pub(super) fn error(body: &mut Vec<Instruction<'static>>) {
    use Instruction::*;
    let message: Vec<_> = "Invalid nominal operation".encode_utf16().collect();
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
        StructNew(DESCRIPTOR),
        GlobalGet(ID_GLOBAL),
        I64Const(1),
        I64Add,
        GlobalSet(ID_GLOBAL),
    ]);
    b.function_with_locals("descriptor-new", &[VALUE], &[VALUE], &[(1, VALUE)], &body);

    let mut body = vec![];
    guard(&mut body, 0, DESCRIPTOR);
    guard(&mut body, 1, ARGS);
    get(&mut body, 0, DESCRIPTOR, 1);
    body.push(LocalSet(2));
    guard(&mut body, 2, ARGS);
    array(&mut body, 2);
    body.push(ArrayLen);
    array(&mut body, 1);
    body.extend([ArrayLen, I32Ne, If(BlockType::Empty)]);
    error(&mut body);
    body.push(End);
    copy_array(&mut body, 1, 2);
    body.extend([
        LocalGet(0),
        RefCastNonNull(HeapType::Concrete(DESCRIPTOR)),
        LocalGet(2),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        I32Const(0),
        RefI31,
        StructNew(OBJECT),
    ]);
    let object_new = b.function_with_locals(
        "object-new",
        &[VALUE, VALUE],
        &[VALUE],
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
    body.extend([LocalGet(0), RefEq, Else, I32Const(0), End]);
    b.function("object-instance", &[VALUE, VALUE], &[ValType::I32], &body);

    let mut body = vec![];
    guard(&mut body, 0, OBJECT);
    get(&mut body, 0, OBJECT, 0);
    let object_descriptor = b.function("object-descriptor", &[VALUE], &[VALUE], &body);

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
        // Reject malformed odd-length tables before pair indexing.
        array(&mut body, table);
        body.extend([ArrayLen, I32Const(1), I32And, If(BlockType::Empty)]);
        error(&mut body);
        body.push(End);
        body.extend([
            I32Const(0),
            LocalSet(index),
            Block(BlockType::Empty),
            Loop(BlockType::Empty),
            LocalGet(index),
        ]);
        array(&mut body, table);
        body.extend([ArrayLen, I32GeU, BrIf(1)]);
        array(&mut body, table);
        body.extend([
            LocalGet(index),
            ArrayGet(ARGS),
            LocalGet(1),
            RefEq,
            If(BlockType::Empty),
        ]);
        array(&mut body, table);
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
            array(&mut body, table);
            body.extend([
                ArrayLen,
                I32Const(i32::MAX - 2),
                I32GtU,
                If(BlockType::Empty),
            ]);
            error(&mut body);
            body.push(End);
            array(&mut body, table);
            body.extend([
                ArrayLen,
                I32Const(2),
                I32Add,
                ArrayNewDefault(ARGS),
                LocalSet(fresh),
            ]);
            array(&mut body, fresh);
            body.push(I32Const(0));
            array(&mut body, table);
            body.push(I32Const(0));
            array(&mut body, table);
            body.extend([
                ArrayLen,
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
            &[(1, ValType::I32), (if writing { 2 } else { 1 }, VALUE)],
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
        RefFunc(ordinary_type),
        I32Const(0),
        I32Const(-1),
        Call(b.names["closure-new"]),
    ]);
    b.function("class-value-new", &[VALUE], &[VALUE], &body);
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
    let source_constructor = invoke_body_with_locals(b, &[(2, ValType::I32), (1, VALUE)], &body);
    let mut body = vec![];
    guard(&mut body, 0, DESCRIPTOR);
    get(&mut body, 0, DESCRIPTOR, 1);
    body.push(LocalSet(1));
    guard(&mut body, 1, ARGS);
    body.extend([
        LocalGet(0),
        RefFunc(source_constructor),
        I32Const(0),
        I32Const(-1),
        Call(b.names["closure-new"]),
    ]);
    b.function_with_locals(
        "source-constructor-new",
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
