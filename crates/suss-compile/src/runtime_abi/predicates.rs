//! Original portable primitive predicate intrinsics; no upstream code is copied.
use super::*;

fn argument(body: &mut Vec<Instruction<'static>>, index: i32) {
    body.extend([
        Instruction::LocalGet(1),
        Instruction::I32Const(index),
        Instruction::ArrayGet(ARGS),
    ]);
}
fn sentinel(body: &mut Vec<Instruction<'static>>, value: i32) {
    body.extend([Instruction::I32Const(value), Instruction::RefI31]);
}
fn boolean(body: &mut Vec<Instruction<'static>>) {
    use Instruction::*;
    // A comparison produces0/1; language booleans are i31 sentinels2/4.
    body.extend([I32Const(2), I32Mul, I32Const(2), I32Add, RefI31]);
}
fn callback(b: &mut Builder, locals: &[(u32, ValType)], body: &[Instruction<'static>]) -> u32 {
    let index = b.count;
    b.functions.function(INVOKE);
    let mut code = Function::new(locals.iter().copied());
    for instruction in body {
        code.instruction(instruction);
    }
    code.instruction(&Instruction::End);
    b.code.function(&code);
    b.count += 1;
    index
}
fn factory(
    b: &mut Builder,
    name: &str,
    arity: i32,
    locals: &[(u32, ValType)],
    body: &[Instruction<'static>],
) -> u32 {
    use Instruction::*;
    let function = callback(b, locals, body);
    b.function(
        name,
        &[],
        &[VALUE],
        &[
            I32Const(0),
            RefI31,
            RefFunc(function),
            I32Const(arity),
            I32Const(arity),
            Call(b.names["closure-new"]),
        ],
    );
    function
}

pub(super) fn functions(b: &mut Builder) -> Vec<u32> {
    use Instruction::*;
    let mut declared = Vec::new();
    for (name, value) in [
        ("predicate-false", 2),
        ("predicate-true", 4),
        ("predicate-undefined", UNDEFINED),
    ] {
        let mut body = vec![];
        argument(&mut body, 0);
        sentinel(&mut body, value);
        body.push(RefEq);
        boolean(&mut body);
        declared.push(factory(b, name, 1, &[], &body));
    }
    let mut body = vec![];
    argument(&mut body, 0);
    sentinel(&mut body, 0);
    body.push(RefEq);
    argument(&mut body, 0);
    sentinel(&mut body, UNDEFINED);
    body.extend([RefEq, I32Or]);
    boolean(&mut body);
    declared.push(factory(b, "predicate-nil", 1, &[], &body));
    for (name, ty) in [
        ("predicate-number", NUMBER),
        ("predicate-string", STRING),
        ("predicate-function", 4),
    ] {
        let mut body = vec![];
        argument(&mut body, 0);
        body.push(RefTestNonNull(HeapType::Concrete(ty)));
        boolean(&mut body);
        declared.push(factory(b, name, 1, &[], &body));
    }

    // JavaScript strict identity compares primitive number/string values, but
    // nominal objects and closures by identity. RefEq alone fails boxed scalars.
    let mut body = vec![];
    argument(&mut body, 0);
    body.push(LocalSet(2));
    argument(&mut body, 1);
    body.push(LocalSet(3));
    body.extend([
        LocalGet(2),
        RefTestNonNull(HeapType::Concrete(NUMBER)),
        LocalGet(3),
        RefTestNonNull(HeapType::Concrete(NUMBER)),
        I32And,
        If(BlockType::Empty),
        LocalGet(2),
        RefCastNonNull(HeapType::Concrete(NUMBER)),
        StructGet {
            struct_type_index: NUMBER,
            field_index: 0,
        },
        LocalGet(3),
        RefCastNonNull(HeapType::Concrete(NUMBER)),
        StructGet {
            struct_type_index: NUMBER,
            field_index: 0,
        },
        F64Eq,
    ]);
    boolean(&mut body);
    body.extend([
        Return,
        End,
        LocalGet(2),
        RefTestNonNull(HeapType::Concrete(STRING)),
        LocalGet(3),
        RefTestNonNull(HeapType::Concrete(STRING)),
        I32And,
        If(BlockType::Empty),
        LocalGet(2),
        RefCastNonNull(HeapType::Concrete(STRING)),
        ArrayLen,
        LocalGet(3),
        RefCastNonNull(HeapType::Concrete(STRING)),
        ArrayLen,
        I32Ne,
        If(BlockType::Empty),
        I32Const(2),
        RefI31,
        Return,
        End,
        I32Const(0),
        LocalSet(4),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(4),
        LocalGet(2),
        RefCastNonNull(HeapType::Concrete(STRING)),
        ArrayLen,
        I32GeU,
        BrIf(1),
        LocalGet(2),
        RefCastNonNull(HeapType::Concrete(STRING)),
        LocalGet(4),
        ArrayGetU(STRING),
        LocalGet(3),
        RefCastNonNull(HeapType::Concrete(STRING)),
        LocalGet(4),
        ArrayGetU(STRING),
        I32Ne,
        If(BlockType::Empty),
        I32Const(2),
        RefI31,
        Return,
        End,
        LocalGet(4),
        I32Const(1),
        I32Add,
        LocalSet(4),
        Br(0),
        End,
        End,
        I32Const(4),
        RefI31,
        Return,
        End,
        LocalGet(2),
        LocalGet(3),
        RefEq,
    ]);
    boolean(&mut body);
    declared.push(factory(
        b,
        "predicate-identical",
        2,
        &[(2, VALUE), (1, ValType::I32)],
        &body,
    ));
    declared
}
