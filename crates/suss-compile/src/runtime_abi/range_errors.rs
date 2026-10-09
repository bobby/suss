//! Original rooted RangeError identity for native array length operations.
use super::*;
use Instruction::*;
pub(super) const DESCRIPTOR_GLOBAL: u32 = primitive_constructors::ARRAY_ROOT + 1;
const CONSTRUCTOR_ROOT: u32 = DESCRIPTOR_GLOBAL + 1;

pub(super) fn append_globals(globals: &mut GlobalSection) {
    globals.global(
        GlobalType {
            val_type: reference(DESCRIPTOR),
            mutable: false,
            shared: false,
        },
        &ConstExpr::extended([
            I64Const(i64::from(numeric::ERROR_GLOBALS) + 4),
            I32Const(0),
            ArrayNewDefault(ARGS),
            I32Const(0),
            ArrayNewDefault(ARGS),
            I32Const(0),
            RefI31,
            I32Const(0),
            RefI31,
            StructNew(DESCRIPTOR),
        ]),
    );
    globals.global(
        GlobalType {
            val_type: VALUE,
            mutable: true,
            shared: false,
        },
        &ConstExpr::extended([I32Const(0), RefI31]),
    );
}
pub(super) fn invalid_length(body: &mut Vec<Instruction<'static>>) {
    let units: Vec<_> = "Invalid array length".encode_utf16().collect();
    body.push(GlobalGet(DESCRIPTOR_GLOBAL));
    body.extend(units.iter().map(|unit| I32Const(*unit as i32)));
    body.extend([
        ArrayNewFixed {
            array_type_index: STRING,
            array_size: units.len() as u32,
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
pub(super) fn functions(b: &mut Builder, callback: u32) -> Vec<u32> {
    b.function(
        "range-error?",
        &[VALUE],
        &[ValType::I32],
        &[
            LocalGet(0),
            RefTestNonNull(HeapType::Concrete(8)),
            If(BlockType::Result(ValType::I32)),
            LocalGet(0),
            RefCastNonNull(HeapType::Concrete(8)),
            StructGet {
                struct_type_index: 8,
                field_index: 0,
            },
            GlobalGet(DESCRIPTOR_GLOBAL),
            RefEq,
            Else,
            I32Const(0),
            End,
        ],
    );
    let mut body = vec![];
    body.extend([
        GlobalGet(CONSTRUCTOR_ROOT),
        RefTestNonNull(HeapType::Concrete(CLOSURE)),
        If(BlockType::Empty),
        GlobalGet(CONSTRUCTOR_ROOT),
        Return,
        End,
        GlobalGet(DESCRIPTOR_GLOBAL),
        RefFunc(callback),
        I32Const(0),
        I32Const(-1),
        Call(b.names["closure-new"]),
        GlobalSet(CONSTRUCTOR_ROOT),
        GlobalGet(CONSTRUCTOR_ROOT),
    ]);
    let units: Vec<_> = "RangeError".encode_utf16().collect();
    body.extend(units.iter().map(|unit| I32Const(*unit as i32)));
    body.extend([
        ArrayNewFixed {
            array_type_index: STRING,
            array_size: units.len() as u32,
        },
        Call(b.names["closure-source-name-initialize"]),
        GlobalGet(CONSTRUCTOR_ROOT),
    ]);
    b.function("range-error-constructor", &[], &[VALUE], &body);
    vec![]
}
