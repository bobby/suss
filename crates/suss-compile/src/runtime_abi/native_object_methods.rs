//! Original bounded Object builtins for owned native objects. These callbacks
//! implement real operations; unsupported boxed primitive domains throw explicitly.
use super::*;
pub(super) const ROOT: u32 = native_objects::TAG_GLOBAL + 1;
fn text(value: &str) -> Vec<Instruction<'static>> {
    let mut code = value
        .encode_utf16()
        .map(|unit| Instruction::I32Const(i32::from(unit)))
        .collect::<Vec<_>>();
    code.push(Instruction::ArrayNewFixed {
        array_type_index: STRING,
        array_size: value.encode_utf16().count() as u32,
    });
    code
}
fn boolean(code: &mut Vec<Instruction<'static>>) {
    use Instruction::*;
    code.extend([I32Const(1), I32Shl, I32Const(2), I32Add, RefI31]);
}
fn callback(b: &mut Builder, locals: &[(u32, ValType)], code: &[Instruction<'static>]) -> u32 {
    let index = b.count;
    b.functions.function(INVOKE);
    let mut body = Function::new(locals.iter().copied());
    for instruction in code {
        body.instruction(instruction);
    }
    body.instruction(&Instruction::End);
    b.code.function(&body);
    b.count += 1;
    index
}
pub(super) fn functions(b: &mut Builder) -> Vec<u32> {
    use Instruction::*;
    let fields = b.names["native-object-fields"];
    let prototype = b.names["native-object-prototype"];
    let new = b.names["native-object-new"];
    let set_proto = b.names["native-object-prototype-set"];
    let set = b.names["native-object-own-define"];
    let mut refs = vec![];
    let mut methods = vec![];
    for name in [
        "toString",
        "valueOf",
        "hasOwnProperty",
        "isPrototypeOf",
        "propertyIsEnumerable",
        "toLocaleString",
        "__defineGetter__",
        "__defineSetter__",
        "__lookupGetter__",
        "__lookupSetter__",
    ] {
        let mut code = vec![LocalGet(1), ArrayLen, I32Eqz, If(BlockType::Empty)];
        nominal::error(&mut code);
        code.extend([End, LocalGet(1), I32Const(0), ArrayGet(ARGS), LocalSet(2)]);
        if name == "toString" {
            for (sentinel, spelling) in [(0, "[object Null]"), (UNDEFINED, "[object Undefined]")] {
                code.extend([
                    LocalGet(2),
                    I32Const(sentinel),
                    RefI31,
                    RefEq,
                    If(BlockType::Empty),
                ]);
                code.extend(text(spelling));
                code.extend([Return, End]);
            }
            code.extend([LocalGet(2), Call(fields), Drop]);
            code.extend(text("[object Object]"));
        } else {
            if name != "isPrototypeOf" {
                code.extend([LocalGet(2), Call(fields), Drop]);
            }
            match name {
                "valueOf" => code.push(LocalGet(2)),
                "hasOwnProperty" => {
                    code.extend([
                        LocalGet(2),
                        LocalGet(1),
                        ArrayLen,
                        I32Const(1),
                        I32GtU,
                        If(BlockType::Result(VALUE)),
                        LocalGet(1),
                        I32Const(1),
                        ArrayGet(ARGS),
                        Else,
                        I32Const(UNDEFINED),
                        RefI31,
                        End,
                        Call(b.names["coerce-string"]),
                        Call(b.names["native-object-own-slot"]),
                        I32Const(0),
                        I32GeS,
                    ]);
                    boolean(&mut code);
                }
                "propertyIsEnumerable" => {
                    code.extend([
                        LocalGet(2),
                        LocalGet(1),
                        ArrayLen,
                        I32Const(1),
                        I32GtU,
                        If(BlockType::Result(VALUE)),
                        LocalGet(1),
                        I32Const(1),
                        ArrayGet(ARGS),
                        Else,
                        I32Const(UNDEFINED),
                        RefI31,
                        End,
                        Call(b.names["coerce-string"]),
                        Call(b.names["native-object-own-descriptor"]),
                        LocalSet(3),
                        LocalGet(3),
                        I32Const(UNDEFINED),
                        RefI31,
                        RefEq,
                        If(BlockType::Result(ValType::I32)),
                        I32Const(0),
                        Else,
                        LocalGet(3),
                        RefCastNonNull(HeapType::Concrete(ARGS)),
                        I32Const(0),
                        ArrayGet(ARGS),
                        RefCastNonNull(HeapType::I31),
                        I31GetU,
                        I32Const(2),
                        I32And,
                        I32Eqz,
                        I32Eqz,
                        End,
                    ]);
                    boolean(&mut code);
                }
                "toLocaleString" => {
                    code.push(LocalGet(2));
                    code.extend(text("toString"));
                    code.extend([
                        Call(b.names["native-object-property-get"]),
                        LocalGet(2),
                        I32Const(0),
                        ArrayNewDefault(ARGS),
                        Call(b.names["object-method-invoke"]),
                    ]);
                }
                "__defineGetter__" | "__defineSetter__" => {
                    code.extend([
                        LocalGet(2),
                        LocalGet(1),
                        ArrayLen,
                        I32Const(1),
                        I32GtU,
                        If(BlockType::Result(VALUE)),
                        LocalGet(1),
                        I32Const(1),
                        ArrayGet(ARGS),
                        Else,
                        I32Const(UNDEFINED),
                        RefI31,
                        End,
                        LocalGet(1),
                        ArrayLen,
                        I32Const(2),
                        I32GtU,
                        If(BlockType::Result(VALUE)),
                        LocalGet(1),
                        I32Const(2),
                        ArrayGet(ARGS),
                        Else,
                        I32Const(UNDEFINED),
                        RefI31,
                        End,
                        I32Const(i32::from(name == "__defineSetter__")),
                        Call(b.names["native-object-legacy-define"]),
                    ]);
                }
                "__lookupGetter__" | "__lookupSetter__" => {
                    code.extend([
                        LocalGet(2),
                        LocalGet(1),
                        ArrayLen,
                        I32Const(1),
                        I32GtU,
                        If(BlockType::Result(VALUE)),
                        LocalGet(1),
                        I32Const(1),
                        ArrayGet(ARGS),
                        Else,
                        I32Const(UNDEFINED),
                        RefI31,
                        End,
                        I32Const(i32::from(name == "__lookupSetter__")),
                        Call(b.names["native-object-legacy-lookup"]),
                    ]);
                }
                "isPrototypeOf" => {
                    code.extend([
                        LocalGet(1),
                        ArrayLen,
                        I32Const(1),
                        I32LeU,
                        If(BlockType::Empty),
                        I32Const(2),
                        RefI31,
                        Return,
                        End,
                        LocalGet(1),
                        I32Const(1),
                        ArrayGet(ARGS),
                        LocalSet(3),
                        LocalGet(3),
                        RefTestNonNull(HeapType::Concrete(7)),
                        I32Eqz,
                        If(BlockType::Empty),
                        LocalGet(3),
                        Call(b.names["coerce-string"]),
                        Drop,
                        I32Const(2),
                        RefI31,
                        Return,
                        End,
                        LocalGet(2),
                        Call(fields),
                        Drop,
                        LocalGet(3),
                        Call(b.names["native-object-check-chain"]),
                        LocalGet(3),
                        Call(prototype),
                        LocalSet(3),
                        Block(BlockType::Empty),
                        Loop(BlockType::Empty),
                        LocalGet(3),
                        I32Const(0),
                        RefI31,
                        RefEq,
                        BrIf(1),
                        LocalGet(3),
                        LocalGet(2),
                        RefEq,
                        If(BlockType::Empty),
                        I32Const(4),
                        RefI31,
                        Return,
                        End,
                        LocalGet(3),
                        Call(prototype),
                        LocalSet(3),
                        Br(0),
                        End,
                        End,
                        I32Const(2),
                        RefI31,
                    ]);
                }
                _ => unreachable!(),
            }
        }
        let anchored = callback(b, &[(2, VALUE)], &code);
        refs.push(anchored);
        let mut detached_code = vec![];
        if name == "toString" {
            detached_code.extend(text("[object Undefined]"));
        } else if name == "isPrototypeOf" {
            detached_code.extend([
                LocalGet(1),
                ArrayLen,
                I32Eqz,
                If(BlockType::Empty),
                I32Const(2),
                RefI31,
                Return,
                End,
                LocalGet(1),
                I32Const(0),
                ArrayGet(ARGS),
                Call(b.names["coerce-string"]),
                Drop,
                I32Const(2),
                RefI31,
            ]);
        } else {
            nominal::error(&mut detached_code);
        }
        let detached = callback(b, &[], &detached_code);
        refs.push(detached);
        methods.push((name, anchored, detached));
    }
    // Object() and Object(null/undefined) create an owned object. Object(owned)
    // returns that same object. Primitive boxing is explicitly unsupported here.
    let mut code = vec![
        LocalGet(1),
        ArrayLen,
        I32Eqz,
        If(BlockType::Result(VALUE)),
        I32Const(0),
        RefI31,
        Else,
        LocalGet(1),
        I32Const(0),
        ArrayGet(ARGS),
        End,
        LocalSet(2),
        LocalGet(2),
        I32Const(0),
        RefI31,
        RefEq,
        LocalGet(2),
        I32Const(UNDEFINED),
        RefI31,
        RefEq,
        I32Or,
        If(BlockType::Result(VALUE)),
    ];
    // Keep the newly allocated receiver in a local before setting its prototype.
    code.extend([
        Call(new),
        LocalTee(2),
        GlobalGet(ROOT),
        Call(set_proto),
        Drop,
        LocalGet(2),
        Else,
        LocalGet(2),
        Call(fields),
        Drop,
        LocalGet(2),
        End,
    ]);
    let constructor = callback(b, &[(1, VALUE)], &code);
    refs.push(constructor);
    let getter = callback(
        b,
        &[],
        &[
            LocalGet(1),
            ArrayLen,
            I32Eqz,
            If(BlockType::Empty),
            // A typed throw for malformed copied callbacks, before array access.
            I32Const(0),
            RefI31,
            Call(fields),
            Drop,
            End,
            LocalGet(1),
            I32Const(0),
            ArrayGet(ARGS),
            Call(prototype),
        ],
    );
    let setter = callback(
        b,
        &[],
        &[
            LocalGet(1),
            ArrayLen,
            I32Eqz,
            If(BlockType::Empty),
            I32Const(0),
            RefI31,
            Call(fields),
            Drop,
            End,
            LocalGet(1),
            I32Const(0),
            ArrayGet(ARGS),
            LocalGet(1),
            ArrayLen,
            I32Const(1),
            I32GtU,
            If(BlockType::Result(VALUE)),
            LocalGet(1),
            I32Const(1),
            ArrayGet(ARGS),
            Else,
            I32Const(UNDEFINED),
            RefI31,
            End,
            Call(b.names["native-object-proto-write"]),
        ],
    );
    let mut detached_code = vec![];
    nominal::error(&mut detached_code);
    let detached = callback(b, &[], &detached_code);
    refs.extend([getter, setter, detached]);
    let mut code = vec![
        GlobalGet(ROOT),
        I32Const(0),
        RefI31,
        RefEq,
        If(BlockType::Empty),
        Call(new),
        LocalSet(0),
    ];
    for (name, anchored, detached) in methods {
        code.push(LocalGet(0));
        code.extend(text(name));
        code.extend([
            GlobalGet(object_methods::TAG_GLOBAL),
            I32Const(0),
            RefI31,
            RefFunc(anchored),
            I32Const(1),
            I32Const(-1),
            Call(b.names["closure-new"]),
            ArrayNewFixed {
                array_type_index: ARGS,
                array_size: 1,
            },
            I32Const(0),
            RefI31,
            StructNew(7),
            RefFunc(detached),
            I32Const(0),
            I32Const(-1),
            Call(b.names["closure-new"]),
            I32Const(5),
            Call(set),
            Drop,
        ]);
    }
    code.push(LocalGet(0));
    code.extend(text("__proto__"));
    for anchored in [getter, setter] {
        code.extend([
            GlobalGet(object_methods::TAG_GLOBAL),
            I32Const(0),
            RefI31,
            RefFunc(anchored),
            I32Const(1),
            I32Const(-1),
            Call(b.names["closure-new"]),
            ArrayNewFixed {
                array_type_index: ARGS,
                array_size: 1,
            },
            I32Const(0),
            RefI31,
            StructNew(7),
            RefFunc(detached),
            I32Const(0),
            I32Const(-1),
            Call(b.names["closure-new"]),
        ]);
    }
    code.extend([
        ArrayNewFixed {
            array_type_index: ARGS,
            array_size: 2,
        },
        I32Const(12),
        Call(set),
        Drop,
    ]);
    code.push(LocalGet(0));
    code.extend(text("constructor"));
    code.extend([
        I32Const(0),
        RefI31,
        RefFunc(constructor),
        I32Const(0),
        I32Const(-1),
        Call(b.names["closure-new"]),
        I32Const(5),
        Call(set),
        Drop,
        LocalGet(0),
        GlobalSet(ROOT),
        End,
        GlobalGet(ROOT),
    ]);
    let root = b.function_with_locals(
        "native-object-default-prototype",
        &[],
        &[VALUE],
        &[(1, VALUE)],
        &code,
    );
    b.function_with_locals(
        "native-object-default-new",
        &[],
        &[VALUE],
        &[(1, VALUE)],
        &[
            Call(root),
            Drop,
            Call(new),
            LocalTee(0),
            GlobalGet(ROOT),
            Call(set_proto),
            Drop,
            LocalGet(0),
        ],
    );
    refs
}
