//! Owned native Error construction and canonical Error-family ancestry.
//! Generic foreign prototypes/accessors remain the explicit host bridge boundary.
use super::*;
use Instruction::*;

pub(super) fn canonical_test_function(b: &mut Builder) {
    let value = 0;
    let mut code = vec![];
    code.extend([
        LocalGet(value),
        RefTestNonNull(HeapType::Concrete(8)),
        If(BlockType::Result(ValType::I32)),
        I32Const(0),
    ]);
    for descriptor in (0..numeric::ERROR_GLOBALS).chain([
        nominal::ERROR_GLOBAL,
        range_errors::DESCRIPTOR_GLOBAL,
        owned_prototypes::TYPE_ERROR_GLOBAL,
    ]) {
        code.extend([
            LocalGet(value),
            RefCastNonNull(HeapType::Concrete(8)),
            StructGet {
                struct_type_index: 8,
                field_index: 0,
            },
            GlobalGet(descriptor),
            RefEq,
            I32Or,
        ]);
    }
    code.extend([Else, I32Const(0), End]);
    b.function(
        "language-error-canonical?",
        &[VALUE],
        &[ValType::I32],
        &code,
    );
}

pub(super) fn family_test(b: &Builder, code: &mut Vec<Instruction<'static>>, value: u32) {
    code.extend([LocalGet(value), Call(b.names["owned-error-ancestry"])]);
}

fn text(code: &mut Vec<Instruction<'static>>, value: &str) {
    code.extend(value.encode_utf16().map(|unit| I32Const(i32::from(unit))));
    code.push(ArrayNewFixed {
        array_type_index: STRING,
        array_size: value.encode_utf16().count() as u32,
    });
}

pub(super) fn callback(b: &mut Builder) -> u32 {
    // Message conversion precedes HasProperty/Get of options.cause. All source
    // operands have already evaluated before invoke; undefined message is absent.
    let mut code = vec![
        ArrayNewFixed {
            array_type_index: STRING,
            array_size: 0,
        },
        LocalSet(2),
        I32Const(0),
        LocalSet(5),
        I32Const(UNDEFINED),
        RefI31,
        LocalSet(4),
        LocalGet(1),
        ArrayLen,
        I32Const(0),
        I32GtU,
        If(BlockType::Empty),
        LocalGet(1),
        I32Const(0),
        ArrayGet(ARGS),
        I32Const(UNDEFINED),
        RefI31,
        RefEq,
        I32Eqz,
        If(BlockType::Empty),
        LocalGet(1),
        I32Const(0),
        ArrayGet(ARGS),
        Call(b.names["coerce-string"]),
        LocalSet(2),
        I32Const(2),
        LocalSet(5),
        End,
        End,
        LocalGet(1),
        ArrayLen,
        I32Const(1),
        I32GtU,
        If(BlockType::Empty),
        LocalGet(1),
        I32Const(1),
        ArrayGet(ARGS),
        LocalSet(3),
        LocalGet(3),
        Call(b.names["native-object?"]),
        If(BlockType::Empty),
        LocalGet(3),
    ];
    text(&mut code, "cause");
    code.extend([
        Call(b.names["native-object-property-descriptor"]),
        I32Const(UNDEFINED),
        RefI31,
        RefEq,
        I32Eqz,
        If(BlockType::Empty),
        LocalGet(3),
    ]);
    text(&mut code, "cause");
    code.extend([
        Call(b.names["native-object-property-get"]),
        LocalSet(4),
        LocalGet(5),
        I32Const(1),
        I32Or,
        LocalSet(5),
        End,
        Else,
        LocalGet(3),
        Call(b.names["source-array?"]),
        If(BlockType::Empty),
        LocalGet(3),
    ]);
    text(&mut code, "cause");
    code.extend([
        Call(b.names["source-array-property-has"]),
        If(BlockType::Empty),
        LocalGet(3),
    ]);
    text(&mut code, "cause");
    code.extend([
        Call(b.names["source-array-property-get"]),
        LocalSet(4),
        LocalGet(5),
        I32Const(1),
        I32Or,
        LocalSet(5),
        End,
        Else,
        LocalGet(3),
        RefTestNonNull(HeapType::Concrete(7)),
        If(BlockType::Empty),
        LocalGet(3),
        RefCastNonNull(HeapType::Concrete(7)),
        StructGet {
            struct_type_index: 7,
            field_index: 0,
        },
        StructGet {
            struct_type_index: DESCRIPTOR,
            field_index: 1,
        },
    ]);
    text(&mut code, "cause");
    code.extend([
        I32Const(1),
        Call(b.names["property-find"]),
        I32Const(0),
        I32GeS,
        If(BlockType::Empty),
        LocalGet(3),
    ]);
    text(&mut code, "cause");
    code.extend([
        Call(b.names["fixed-named-property-get"]),
        LocalSet(4),
        LocalGet(5),
        I32Const(1),
        I32Or,
        LocalSet(5),
        Else,
        LocalGet(3),
        RefCastNonNull(HeapType::Concrete(7)),
        StructGet {
            struct_type_index: 7,
            field_index: 2,
        },
        LocalSet(6),
        LocalGet(6),
        RefTestNonNull(HeapType::Concrete(ARGS)),
        If(BlockType::Empty),
        LocalGet(6),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        ArrayLen,
        I32Const(1),
        I32Ne,
        If(BlockType::Empty),
    ]);
    nominal::error(&mut code);
    code.extend([
        End,
        LocalGet(6),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        I32Const(0),
        ArrayGet(ARGS),
        LocalSet(6),
        LocalGet(6),
        I32Const(0),
        RefI31,
        RefEq,
        I32Eqz,
        If(BlockType::Empty),
        LocalGet(6),
    ]);
    text(&mut code, "cause");
    code.extend([
        Call(b.names["native-object-property-descriptor"]),
        LocalSet(7),
        LocalGet(7),
        I32Const(UNDEFINED),
        RefI31,
        RefEq,
        I32Eqz,
        If(BlockType::Empty),
        LocalGet(6),
        LocalGet(3),
    ]);
    text(&mut code, "cause");
    code.extend([
        Call(b.names["native-object-property-get-with-receiver"]),
        LocalSet(4),
        LocalGet(5),
        I32Const(1),
        I32Or,
        LocalSet(5),
        End,
        End,
        End,
        End,
        Else,
        LocalGet(3),
        RefTestNonNull(HeapType::Concrete(8)),
        If(BlockType::Empty),
        LocalGet(3),
        Call(b.names["language-error-canonical?"]),
        I32Eqz,
        If(BlockType::Empty),
    ]);
    nominal::error(&mut code);
    code.extend([
        End,
        LocalGet(3),
        RefCastNonNull(HeapType::Concrete(8)),
        StructGet {
            struct_type_index: 8,
            field_index: 2,
        },
        RefCastNonNull(HeapType::I31),
        I31GetU,
        I32Const(1),
        I32And,
        If(BlockType::Empty),
        LocalGet(3),
        RefCastNonNull(HeapType::Concrete(8)),
        StructGet {
            struct_type_index: 8,
            field_index: 3,
        },
        LocalSet(4),
        LocalGet(5),
        I32Const(1),
        I32Or,
        LocalSet(5),
        End,
        Else,
        LocalGet(3),
        RefTestNonNull(HeapType::Concrete(CLOSURE)),
        If(BlockType::Empty),
    ]);
    // Callable options are objects too. HasProperty on the owned data-property
    // table precedes reading cause; absence is distinct from an own nil/false.
    code.extend([
        LocalGet(3),
        Call(b.names["closure-property-fields"]),
        LocalSet(6),
        LocalGet(6),
        RefTestNonNull(HeapType::Concrete(ARGS)),
        I32Eqz,
        If(BlockType::Empty),
    ]);
    nominal::error(&mut code);
    code.extend([
        End,
        LocalGet(6),
        RefCastNonNull(HeapType::Concrete(ARGS)),
        I32Const(1),
        ArrayGet(ARGS),
    ]);
    text(&mut code, "cause");
    code.extend([
        I32Const(2),
        Call(b.names["property-find"]),
        I32Const(0),
        I32GeS,
        If(BlockType::Empty),
        LocalGet(3),
    ]);
    text(&mut code, "cause");
    code.extend([
        Call(b.names["fixed-named-property-get"]),
        LocalSet(4),
        LocalGet(5),
        I32Const(1),
        I32Or,
        LocalSet(5),
        End,
    ]);
    code.extend([
        End,
        End,
        End,
        End,
        End,
        End,
        Call(b.names["native-object-new"]),
        LocalSet(8),
        LocalGet(8),
        LocalGet(0),
        Call(b.names["owned-descriptor-prototype"]),
        Call(b.names["native-object-prototype-set"]),
        Drop,
    ]);
    for (key, bit, value) in [("message", 2, 2), ("cause", 1, 4)] {
        code.extend([
            LocalGet(5),
            I32Const(bit),
            I32And,
            If(BlockType::Empty),
            LocalGet(8),
        ]);
        text(&mut code, key);
        code.extend([
            LocalGet(value),
            I32Const(5),
            Call(b.names["native-object-own-define"]),
            Drop,
            End,
        ]);
    }
    code.extend([
        LocalGet(0),
        RefCastNonNull(HeapType::Concrete(DESCRIPTOR)),
        LocalGet(2),
        RefCastNonNull(HeapType::Concrete(STRING)),
        LocalGet(5),
        RefI31,
        LocalGet(8),
        ArrayNewFixed {
            array_type_index: ARGS,
            array_size: 2,
        },
        LocalGet(4),
        I32Const(0),
        RefI31,
        StructNew(8),
    ]);
    let index = b.count;
    b.functions.function(INVOKE);
    let mut function = Function::new([(3, VALUE), (1, ValType::I32), (3, VALUE)]);
    for instruction in code {
        function.instruction(&instruction);
    }
    function.instruction(&End);
    b.code.function(&function);
    b.count += 1;
    index
}

pub(super) fn functions(b: &mut Builder, callback: u32) {
    b.function(
        "undefined-value",
        &[],
        &[VALUE],
        &[I32Const(UNDEFINED), RefI31],
    );
    b.function(
        "nan-value",
        &[],
        &[VALUE],
        &[F64Const(f64::NAN.into()), StructNew(NUMBER)],
    );
    // Descriptor-owned cache roots exactly the canonical constructor, with no
    // instance registry. RangeError retains its existing canonical root.
    for (descriptor, export, name) in [
        (nominal::ERROR_GLOBAL, "error-constructor", "Error"),
        (
            owned_prototypes::TYPE_ERROR_GLOBAL,
            "type-error-constructor",
            "TypeError",
        ),
    ] {
        let mut code = vec![
            GlobalGet(descriptor),
            GlobalGet(type_values::KEY_GLOBAL),
            Call(b.names["protocol-method-get"]),
            LocalSet(0),
            LocalGet(0),
            RefTestNonNull(HeapType::Concrete(CLOSURE)),
            If(BlockType::Empty),
            LocalGet(0),
            Return,
            End,
            GlobalGet(descriptor),
            RefFunc(callback),
            I32Const(0),
            I32Const(-1),
            Call(b.names["closure-new"]),
            LocalSet(0),
            GlobalGet(descriptor),
            GlobalGet(type_values::KEY_GLOBAL),
            LocalGet(0),
            Call(b.names["protocol-method-set"]),
            LocalGet(0),
        ];
        text(&mut code, name);
        code.extend([Call(b.names["closure-source-name-initialize"]), LocalGet(0)]);
        b.function_with_locals(export, &[], &[VALUE], &[(1, VALUE)], &code);
    }
    let mut code = vec![];
    let mut plan = vec![];
    for (descriptor, factory) in [
        (nominal::ERROR_GLOBAL, "error-constructor"),
        (range_errors::DESCRIPTOR_GLOBAL, "range-error-constructor"),
        (
            owned_prototypes::TYPE_ERROR_GLOBAL,
            "type-error-constructor",
        ),
        (owned_prototypes::OBJECT_DESCRIPTOR, "object-constructor"),
    ] {
        let branch = [
            LocalGet(0),
            GlobalGet(descriptor),
            RefEq,
            If(BlockType::Empty),
            Call(b.names[factory]),
            Return,
            End,
        ];
        code.extend(branch.iter().cloned());
        plan.extend(branch);
    }
    code.extend([LocalGet(0), Call(b.names["nominal-source-constructor-new"])]);
    b.function("source-constructor-new", &[VALUE], &[VALUE], &code);
    plan.extend([LocalGet(0), Call(b.names["nominal-source-constructor-plan"])]);
    b.function("source-constructor-plan", &[VALUE], &[VALUE], &plan);
    b.function(
        "source-construct-from-plan", &[VALUE, reference(ARGS)], &[VALUE],
        &[
            LocalGet(0), RefTestNonNull(HeapType::Concrete(DESCRIPTOR)),
            If(BlockType::Result(VALUE)),
            LocalGet(0), LocalGet(1), Call(b.names["nominal-source-object-new"]),
            Else,
            LocalGet(0), LocalGet(1), Call(b.names["invoke"]),
            End,
        ],
    );
}
