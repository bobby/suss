//! Original ordered OrdinaryToPrimitive adapter. Typed immutable function roots
//! resolve the numeric/storage dependency cycle without a host object registry.
use super::*;
use Instruction::*;
const NUMBER_ROOT: u32 = range_errors::DESCRIPTOR_GLOBAL + 3;
const STRING_ROOT: u32 = NUMBER_ROOT + 1;
const PRIMITIVE_ROOT: u32 = STRING_ROOT + 1;
#[derive(Clone, Copy)]
pub(super) struct Types {
    number: u32,
    string: u32,
    primitive: u32,
}
pub(super) fn declare(b: &mut Builder) -> Types {
    let types = Types {
        number: b.next_type,
        string: b.next_type + 1,
        primitive: b.next_type + 2,
    };
    b.types.ty().function([VALUE], [ValType::F64]);
    b.types.ty().function([VALUE], [VALUE]);
    b.types.ty().function([VALUE, ValType::I32], [VALUE]);
    b.next_type += 3;
    types
}
pub(super) fn number(code: &mut Vec<Instruction<'static>>, types: Types) {
    code.extend([LocalGet(0), GlobalGet(NUMBER_ROOT), CallRef(types.number)]);
}
pub(super) fn string(code: &mut Vec<Instruction<'static>>, types: Types) {
    code.extend([LocalGet(0), GlobalGet(STRING_ROOT), CallRef(types.string)]);
}
pub(super) fn primitive(code: &mut Vec<Instruction<'static>>, types: Types, local: u32, hint: i32) {
    code.extend([
        LocalGet(local),
        I32Const(hint),
        GlobalGet(PRIMITIVE_ROOT),
        CallRef(types.primitive),
        LocalSet(local),
    ]);
}
fn text(code: &mut Vec<Instruction<'static>>, s: &str) {
    code.extend(s.encode_utf16().map(|u| I32Const(i32::from(u))));
    code.push(ArrayNewFixed {
        array_type_index: STRING,
        array_size: s.encode_utf16().count() as u32,
    });
}
pub(super) fn functions(b: &mut Builder) -> [u32; 3] {
    let mut code = vec![
        LocalGet(0),
        RefTestNonNull(HeapType::Concrete(NUMBER)),
        LocalGet(0),
        RefTestNonNull(HeapType::Concrete(STRING)),
        I32Or,
    ];
    for sentinel in [0, 2, 4, UNDEFINED] {
        code.extend([LocalGet(0), I32Const(sentinel), RefI31, RefEq, I32Or]);
    }
    let is_primitive = b.function("coercion-primitive?", &[VALUE], &[ValType::I32], &code);
    code = vec![
        LocalGet(0),
        Call(is_primitive),
        If(BlockType::Empty),
        LocalGet(0),
        Return,
        End,
        LocalGet(0),
        RefTestNonNull(HeapType::Concrete(7)),
        LocalGet(0),
        RefTestNonNull(HeapType::Concrete(CLOSURE)),
        I32Or,
        I32Eqz,
        If(BlockType::Empty),
    ];
    nominal::error(&mut code);
    code.push(End);
    // Number/default hint uses valueOf then toString; string hint reverses them.
    // Look up the second property only after the first call has completed.
    for second in [false, true] {
        code.extend([
            LocalGet(0),
            LocalGet(1),
            I32Eqz,
            If(BlockType::Result(VALUE)),
        ]);
        text(&mut code, if second { "toString" } else { "valueOf" });
        code.push(Else);
        text(&mut code, if second { "valueOf" } else { "toString" });
        code.extend([
            End,
            Call(b.names["named-property-get"]),
            LocalTee(2),
            RefTestNonNull(HeapType::Concrete(CLOSURE)),
            If(BlockType::Empty),
            LocalGet(2),
            LocalGet(0),
            ArrayNewFixed {
                array_type_index: ARGS,
                array_size: 0,
            },
            Call(b.names["object-method-invoke"]),
            LocalTee(3),
            Call(is_primitive),
            If(BlockType::Empty),
            LocalGet(3),
            Return,
            End,
            End,
        ]);
    }
    nominal::error(&mut code);
    let primitive = b.function_with_locals(
        "object-to-primitive",
        &[VALUE, ValType::I32],
        &[VALUE],
        &[(2, VALUE)],
        &code,
    );
    let number = b.function(
        "object-coerce-number",
        &[VALUE],
        &[ValType::F64],
        &[
            LocalGet(0),
            I32Const(0),
            Call(primitive),
            Call(b.names["coerce-number"]),
        ],
    );
    let string = b.function(
        "object-coerce-string",
        &[VALUE],
        &[VALUE],
        &[
            LocalGet(0),
            I32Const(1),
            Call(primitive),
            Call(b.names["coerce-string"]),
        ],
    );
    [number, string, primitive]
}
pub(super) fn append_globals(globals: &mut GlobalSection, types: Types, functions: [u32; 3]) {
    for (ty, function) in [types.number, types.string, types.primitive]
        .into_iter()
        .zip(functions)
    {
        globals.global(
            GlobalType {
                val_type: reference(ty),
                mutable: false,
                shared: false,
            },
            &ConstExpr::extended([RefFunc(function)]),
        );
    }
}
