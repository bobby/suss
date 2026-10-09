//! Original portable scalar constructor values for the type adapter.
//! Canonical rooted closures perform scalar conversions; no kind-marker values.
//! Object ToPrimitive remains the existing explicit conversion boundary.
use super::*;
use Instruction::*;

pub(super) const NUMBER_ROOT: u32 = type_values::KEY_GLOBAL + 1;
pub(super) const STRING_ROOT: u32 = NUMBER_ROOT + 1;
pub(super) const BOOLEAN_ROOT: u32 = STRING_ROOT + 1;
pub(super) const ARRAY_ROOT: u32 = BOOLEAN_ROOT + 1;

pub(super) fn append_globals(globals: &mut GlobalSection) {
    for _ in 0..4 {
        globals.global(
            GlobalType {
                val_type: VALUE,
                mutable: true,
                shared: false,
            },
            &ConstExpr::extended([I32Const(0), RefI31]),
        );
    }
}
fn boolean(body: &mut Vec<Instruction<'static>>) {
    body.extend([I32Const(2), I32Mul, I32Const(2), I32Add, RefI31]);
}
fn callback(b: &mut Builder, body: &[Instruction<'static>]) -> u32 {
    let index = b.count;
    b.functions.function(INVOKE);
    let mut function = Function::new([(1, VALUE), (1, ValType::F64)]);
    for instruction in body {
        function.instruction(instruction);
    }
    function.instruction(&End);
    b.code.function(&function);
    b.count += 1;
    index
}

pub(super) fn functions(b: &mut Builder) -> Vec<u32> {
    let mut declared = Vec::new();
    for (export, root, label) in [
        ("number-constructor", NUMBER_ROOT, "Number"),
        ("string-constructor", STRING_ROOT, "String"),
        ("boolean-constructor", BOOLEAN_ROOT, "Boolean"),
    ] {
        // Native scalar constructor ordinary calls accept zero or more arguments;
        // all source argument effects run, only the first value is converted.
        let mut body = vec![LocalGet(1), ArrayLen, I32Eqz, If(BlockType::Empty)];
        match root {
            NUMBER_ROOT => body.extend([F64Const(0.0.into()), Call(b.names["number-box"])]),
            STRING_ROOT => body.push(ArrayNewFixed {
                array_type_index: STRING,
                array_size: 0,
            }),
            _ => body.extend([I32Const(2), RefI31]),
        }
        body.extend([
            Return,
            End,
            LocalGet(1),
            I32Const(0),
            ArrayGet(ARGS),
            LocalSet(2),
        ]);
        match root {
            NUMBER_ROOT => body.extend([LocalGet(2), Call(b.names["primitive-f64-coerce"])]),
            STRING_ROOT => body.extend([LocalGet(2), Call(b.names["coerce-string"])]),
            _ => {
                body.extend([
                    LocalGet(2),
                    RefTestNonNull(HeapType::Concrete(NUMBER)),
                    If(BlockType::Empty),
                    LocalGet(2),
                    RefCastNonNull(HeapType::Concrete(NUMBER)),
                    StructGet {
                        struct_type_index: NUMBER,
                        field_index: 0,
                    },
                    LocalSet(3),
                    LocalGet(3),
                    F64Const(0.0.into()),
                    F64Ne,
                    LocalGet(3),
                    LocalGet(3),
                    F64Eq,
                    I32And,
                ]);
                boolean(&mut body);
                body.extend([
                    Return,
                    End,
                    LocalGet(2),
                    RefTestNonNull(HeapType::Concrete(STRING)),
                    If(BlockType::Empty),
                    LocalGet(2),
                    RefCastNonNull(HeapType::Concrete(STRING)),
                    ArrayLen,
                    I32Const(0),
                    I32Ne,
                ]);
                boolean(&mut body);
                body.extend([Return, End]);
                for sentinel in [0, 2, UNDEFINED] {
                    body.extend([LocalGet(2), I32Const(sentinel), RefI31, RefEq]);
                }
                body.extend([I32Or, I32Or, I32Eqz]);
                boolean(&mut body);
            }
        }
        let invoke = callback(b, &body);
        declared.push(invoke);
        let mut body = vec![
            GlobalGet(root),
            RefTestNonNull(HeapType::Concrete(CLOSURE)),
            If(BlockType::Empty),
            GlobalGet(root),
            Return,
            End,
            ArrayNewFixed {
                array_type_index: ARGS,
                array_size: 0,
            },
            RefFunc(invoke),
            I32Const(0),
            I32Const(-1),
            Call(b.names["closure-new"]),
            GlobalSet(root),
            GlobalGet(root),
        ];
        let units: Vec<_> = label.encode_utf16().collect();
        body.extend(units.iter().map(|unit| I32Const(*unit as i32)));
        body.extend([
            ArrayNewFixed {
                array_type_index: STRING,
                array_size: units.len() as u32,
            },
            Call(b.names["closure-source-name-initialize"]),
            GlobalGet(root),
        ]);
        b.function(export, &[], &[VALUE], &body);
    }
    let invoke = callback(
        b,
        &[LocalGet(1), Call(b.names["source-array-constructor-args"])],
    );
    declared.push(invoke);
    let mut body = vec![
        GlobalGet(ARRAY_ROOT),
        RefTestNonNull(HeapType::Concrete(CLOSURE)),
        If(BlockType::Empty),
        GlobalGet(ARRAY_ROOT),
        Return,
        End,
        ArrayNewFixed {
            array_type_index: ARGS,
            array_size: 0,
        },
        RefFunc(invoke),
        I32Const(0),
        I32Const(-1),
        Call(b.names["closure-new"]),
        GlobalSet(ARRAY_ROOT),
        GlobalGet(ARRAY_ROOT),
    ];
    let units: Vec<_> = "Array".encode_utf16().collect();
    body.extend(units.iter().map(|unit| I32Const(*unit as i32)));
    body.extend([
        ArrayNewFixed {
            array_type_index: STRING,
            array_size: units.len() as u32,
        },
        Call(b.names["closure-source-name-initialize"]),
        GlobalGet(ARRAY_ROOT),
    ]);
    b.function("array-constructor", &[], &[VALUE], &body);
    declared
}
