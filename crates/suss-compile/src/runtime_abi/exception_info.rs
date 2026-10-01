//! Original descriptor-backed ExceptionInfo core closures; no JS runtime dependency.
use super::*;
pub(super) const DESCRIPTOR_GLOBAL: u32 = dynamic::CURRENT + 1;
pub(super) const ORDINARY_ROOT: u32 = DESCRIPTOR_GLOBAL + 1;
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
fn argument(body: &mut Vec<Instruction<'static>>, index: i32) {
    body.extend([
        Instruction::LocalGet(1),
        Instruction::I32Const(index),
        Instruction::ArrayGet(ARGS),
    ]);
}
fn malformed(body: &mut Vec<Instruction<'static>>) {
    use Instruction::*;
    body.push(GlobalGet(nominal::ERROR_GLOBAL));
    let text: Vec<_> = "Invalid ExceptionInfo storage".encode_utf16().collect();
    body.extend(text.iter().map(|x| I32Const(*x as i32)));
    body.extend([
        ArrayNewFixed {
            array_type_index: STRING,
            array_size: text.len() as u32,
        },
        I32Const(0),
        RefI31,
        I32Const(0),
        RefI31,
        I32Const(0), RefI31, StructNew(8),
        Throw(0),
    ]);
}
fn named_field(b: &mut Builder, field: &str) -> u32 {
    use Instruction::*;
    let mut body = vec![
        LocalGet(0),
        RefCastNonNull(HeapType::Concrete(7)),
        StructGet {
            struct_type_index: 7,
            field_index: 1,
        },
        LocalSet(1),
        LocalGet(0),
        RefCastNonNull(HeapType::Concrete(7)),
        StructGet {
            struct_type_index: 7,
            field_index: 0,
        },
        StructGet {
            struct_type_index: DESCRIPTOR,
            field_index: 1,
        },
        LocalSet(2),
        LocalGet(2),
        RefTestNonNull(HeapType::Concrete(ARGS)),
        I32Eqz,
        If(BlockType::Empty),
    ];
    malformed(&mut body);
    body.push(End);
    body.extend([
        LocalGet(1),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        ArrayLen,
        LocalGet(2),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        ArrayLen,
        I32Ne,
        If(BlockType::Empty),
    ]);
    malformed(&mut body);
    body.push(End);
    body.extend([
        I32Const(0),
        LocalSet(4),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(4),
        LocalGet(2),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        ArrayLen,
        I32GeU,
        BrIf(1),
        LocalGet(2),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        LocalGet(4),
        ArrayGet(ARGS),
        LocalSet(3),
        LocalGet(3),
        RefTestNonNull(HeapType::Concrete(STRING)),
        I32Eqz,
        If(BlockType::Empty),
    ]);
    malformed(&mut body);
    body.push(End);
    let units: Vec<_> = field.encode_utf16().collect();
    body.extend([
        LocalGet(3),
        RefCastNonNull(HeapType::Concrete(STRING)),
        ArrayLen,
        I32Const(units.len() as i32),
        I32Eq,
        If(BlockType::Empty),
        I32Const(1),
    ]);
    for (i, unit) in units.into_iter().enumerate() {
        body.extend([
            LocalGet(3),
            RefCastNonNull(HeapType::Concrete(STRING)),
            I32Const(i as i32),
            ArrayGetU(STRING),
            I32Const(unit as i32),
            I32Eq,
            I32And,
        ]);
    }
    body.extend([
        If(BlockType::Empty),
        LocalGet(1),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        LocalGet(4),
        ArrayGet(ARGS),
        Return,
        End,
        End,
        LocalGet(4),
        I32Const(1),
        I32Add,
        LocalSet(4),
        Br(0),
        End,
        End,
        I32Const(UNDEFINED),
        RefI31,
    ]);
    b.function_with_locals(
        &format!("exception-info-field-{field}"),
        &[VALUE],
        &[VALUE],
        &[(3, VALUE), (1, ValType::I32)],
        &body,
    )
}
pub(super) fn functions(b: &mut Builder) -> Vec<u32> {
    use Instruction::*;
    let mut declared = vec![];
    // The pinned constructor explicitly returns `this`; an ordinary core call
    // has one rooted realm object, unlike deftype's undefined ordinary result.
    let mut ordinary_body = vec![];
    for index in 0..3 {
        ordinary_body.extend([
            GlobalGet(ORDINARY_ROOT),
            StructGet {
                struct_type_index: 7,
                field_index: 1,
            },
            I32Const(index),
            LocalGet(1),
            ArrayLen,
            I32Const(index),
            I32GtU,
            If(BlockType::Result(VALUE)),
        ]);
        argument(&mut ordinary_body, index);
        ordinary_body.extend([Else, I32Const(UNDEFINED), RefI31, End, ArraySet(ARGS)]);
    }
    ordinary_body.push(GlobalGet(ORDINARY_ROOT));
    let ordinary = callback(b, &[], &ordinary_body);
    declared.push(ordinary);
    b.function(
        "core-exception-info-class",
        &[],
        &[VALUE],
        &[
            GlobalGet(DESCRIPTOR_GLOBAL),
            RefFunc(ordinary),
            I32Const(0),
            I32Const(-1),
            Call(b.names["closure-new"]),
        ],
    );
    let binding_get = b.names["binding-get"];
    let constructor_descriptor = b.names["constructor-descriptor"];
    let source_constructor = b.names["source-constructor-new"];
    let invoke = b.names["invoke"];
    // The pinned two-argument overload calls the current three-argument
    // binding, including when this original function value escaped a redefinition.
    let mut body = vec![
        LocalGet(1),
        ArrayLen,
        I32Const(2),
        I32Eq,
        If(BlockType::Empty),
        LocalGet(0),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        I32Const(1),
        ArrayGet(ARGS),
        Call(binding_get),
    ];
    argument(&mut body, 0);
    argument(&mut body, 1);
    body.extend([
        I32Const(0),
        RefI31,
        ArrayNewFixed {
            array_type_index: ARGS,
            array_size: 3,
        },
        Call(invoke),
        Return,
        End,
    ]);
    body.extend([
        LocalGet(0),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        I32Const(0),
        ArrayGet(ARGS),
        Call(binding_get),
        Call(constructor_descriptor),
        Call(source_constructor),
    ]);
    argument(&mut body, 0);
    argument(&mut body, 1);
    body.extend([
        LocalGet(1),
        ArrayLen,
        I32Const(3),
        I32Eq,
        If(BlockType::Result(VALUE)),
    ]);
    argument(&mut body, 2);
    body.extend([
        Else,
        I32Const(0),
        RefI31,
        End,
        ArrayNewFixed {
            array_type_index: ARGS,
            array_size: 3,
        },
        Call(invoke),
    ]);
    let make = callback(b, &[], &body);
    declared.push(make);
    b.function(
        "core-ex-info",
        &[VALUE, VALUE],
        &[VALUE],
        &[
            LocalGet(0),
            LocalGet(1),
            ArrayNewFixed {
                array_type_index: ARGS,
                array_size: 2,
            },
            RefFunc(make),
            I32Const(2),
            I32Const(3),
            Call(b.names["closure-new"]),
        ],
    );
    for (name, field) in [("ex-message", 0), ("ex-data", 1), ("ex-cause", 2)] {
        let property = named_field(
            b,
            match field {
                0 => "message",
                1 => "data",
                _ => "cause",
            },
        );
        let mut body = vec![];
        if field != 0 {
            body.extend([
                LocalGet(0),
                Call(binding_get),
                Call(constructor_descriptor),
                LocalSet(3),
            ]);
        }
        argument(&mut body, 0);
        body.extend([
            LocalSet(2),
            LocalGet(2),
            RefTestNonNull(HeapType::Concrete(7)),
            If(BlockType::Empty),
            LocalGet(2),
            RefCastNonNull(HeapType::Concrete(7)),
            StructGet {
                struct_type_index: 7,
                field_index: 0,
            },
            if field == 0 {
                GlobalGet(DESCRIPTOR_GLOBAL)
            } else {
                LocalGet(3)
            },
            RefEq,
            If(BlockType::Empty),
            LocalGet(2),
            Call(property),
            Return,
            End,
            End,
        ]);
        if field == 0 {
            body.extend([
                LocalGet(2),
                RefTestNonNull(HeapType::Concrete(8)),
                If(BlockType::Empty),
                LocalGet(2),
                RefCastNonNull(HeapType::Concrete(8)),
                StructGet {
                    struct_type_index: 8,
                    field_index: 1,
                },
                Return,
                End,
            ]);
        }
        body.extend([I32Const(0), RefI31]);
        let getter = callback(b, &[(2, VALUE)], &body);
        declared.push(getter);
        b.function(
            &format!("core-{name}"),
            &[VALUE],
            &[VALUE],
            &[
                LocalGet(0),
                RefFunc(getter),
                I32Const(1),
                I32Const(1),
                Call(b.names["closure-new"]),
            ],
        );
    }
    declared
}
