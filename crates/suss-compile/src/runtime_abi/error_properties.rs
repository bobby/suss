//! Owned Error methods. Property storage stays rooted in the actual Error;
//! callbacks receive the original physical receiver, not the backing owner.
use super::*;
use Instruction::*;

pub(super) fn text(code: &mut Vec<Instruction<'static>>, value: &str) {
    code.extend(value.encode_utf16().map(|unit| I32Const(i32::from(unit))));
    code.push(ArrayNewFixed {
        array_type_index: STRING,
        array_size: value.encode_utf16().count() as u32,
    });
}
pub(super) fn type_error(b: &Builder, code: &mut Vec<Instruction<'static>>, message: &str) {
    code.push(Call(b.names["type-error-constructor"]));
    text(code, message);
    code.extend([
        ArrayNewFixed {
            array_type_index: ARGS,
            array_size: 1,
        },
        Call(b.names["invoke"]),
        Throw(0),
    ]);
}
pub(super) fn callback(
    b: &mut Builder,
    locals: &[(u32, ValType)],
    code: &[Instruction<'static>],
) -> u32 {
    let index = b.count;
    b.functions.function(INVOKE);
    let mut function = Function::new(locals.iter().copied());
    for instruction in code {
        function.instruction(instruction);
    }
    function.instruction(&End);
    b.code.function(&function);
    b.count += 1;
    index
}
pub(super) fn functions(b: &mut Builder) -> Vec<u32> {
    let typed_throw = b.function(
        "owned-type-error-implementation",
        &[VALUE],
        &[VALUE],
        &[
            Call(b.names["type-error-constructor"]),
            LocalGet(0),
            ArrayNewFixed {
                array_type_index: ARGS,
                array_size: 1,
            },
            Call(b.names["invoke"]),
            Throw(0),
        ],
    );
    // ES Error.prototype.toString: get/convert name completely before touching
    // message; absent values have different defaults from nil/false/empty.
    let mut code = vec![LocalGet(1), ArrayLen, I32Eqz, If(BlockType::Empty)];
    type_error(
        b,
        &mut code,
        "Method Error.prototype.toString called on incompatible receiver undefined",
    );
    code.extend([
        End,
        LocalGet(1),
        I32Const(0),
        ArrayGet(ARGS),
        LocalSet(2),
        LocalGet(2),
        RefTestNonNull(HeapType::Concrete(7)),
        LocalGet(2),
        RefTestNonNull(HeapType::Concrete(CLOSURE)),
        I32Or,
        LocalGet(2),
        Call(b.names["language-error-canonical?"]),
        I32Or,
        I32Eqz,
        If(BlockType::Empty),
    ]);
    // Receiver diagnostics contain the primitive spelling, exactly as Node's
    // pinned oracle. ToString here is safe: only primitive receivers reach it.
    code.push(Call(b.names["type-error-constructor"]));
    text(
        &mut code,
        "Method Error.prototype.toString called on incompatible receiver ",
    );
    code.extend([
        LocalGet(2),
        Call(b.names["coerce-string"]),
        Call(b.names["string-concat"]),
        ArrayNewFixed {
            array_type_index: ARGS,
            array_size: 1,
        },
        Call(b.names["invoke"]),
        Throw(0),
        End,
    ]);
    for (key, default, local) in [("name", "Error", 3), ("message", "", 4)] {
        code.push(LocalGet(2));
        text(&mut code, key);
        code.extend([
            Call(b.names["named-property-get"]),
            LocalSet(local),
            LocalGet(local),
            I32Const(UNDEFINED),
            RefI31,
            RefEq,
            If(BlockType::Result(VALUE)),
        ]);
        text(&mut code, default);
        code.extend([
            Else,
            LocalGet(local),
            Call(b.names["coerce-string"]),
            End,
            LocalSet(local),
        ]);
    }
    for (empty, other) in [(3, 4), (4, 3)] {
        code.extend([
            LocalGet(empty),
            RefCastNonNull(HeapType::Concrete(STRING)),
            ArrayLen,
            I32Eqz,
            If(BlockType::Empty),
            LocalGet(other),
            Return,
            End,
        ]);
    }
    code.push(LocalGet(3));
    text(&mut code, ": ");
    code.extend([
        Call(b.names["string-concat"]),
        LocalGet(4),
        Call(b.names["string-concat"]),
    ]);
    let anchored = callback(b, &[(3, VALUE)], &code);
    let mut detached_code = vec![];
    type_error(
        b,
        &mut detached_code,
        "Method Error.prototype.toString called on incompatible receiver null",
    );
    let detached = callback(b, &[], &detached_code);
    b.function(
        "error-to-string-method",
        &[],
        &[VALUE],
        &[
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
            I32Const(0),
            RefI31,
            StructNew(7),
            RefFunc(detached),
            I32Const(0),
            I32Const(-1),
            Call(b.names["closure-new"]),
        ],
    );
    vec![anchored, detached, typed_throw]
}
