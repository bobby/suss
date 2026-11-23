//! Original native MathContext constructor overloads and UTF-16 text grammar.
use super::*;

const DIGIT_STARTS: [i32; 37] = [
    48, 1632, 1776, 1984, 2406, 2534, 2662, 2790, 2918, 3046, 3174, 3302, 3430, 3558, 3664, 3792,
    3872, 4160, 4240, 6112, 6160, 6470, 6608, 6784, 6800, 6992, 7088, 7232, 7248, 42528, 43216,
    43264, 43472, 43504, 43600, 44016, 65296,
];
const MODES: [&str; 8] = [
    "UP",
    "DOWN",
    "CEILING",
    "FLOOR",
    "HALF_UP",
    "HALF_DOWN",
    "HALF_EVEN",
    "UNNECESSARY",
];
fn string_unit(index: Vec<Instruction<'static>>) -> Vec<Instruction<'static>> {
    let mut code = vec![LocalGet(0), RefCastNonNull(HeapType::Concrete(STRING))];
    code.extend(index);
    code.push(ArrayGetU(STRING));
    code
}
fn matches_text(offset: Option<u32>, expected: &str) -> Vec<Instruction<'static>> {
    let mut code = vec![I32Const(1)];
    for (i, unit) in expected.encode_utf16().enumerate() {
        let index = match offset {
            Some(local) => vec![LocalGet(local), I32Const(i as i32), I32Add],
            None => vec![I32Const(i as i32)],
        };
        code.extend(string_unit(index));
        code.extend([I32Const(i32::from(unit)), I32Eq, I32And]);
    }
    code
}
fn bad_if(code: &mut Vec<Instruction<'static>>) {
    code.push(If(BlockType::Empty));
    code.extend(error("bad string format"));
    code.push(End);
}
fn text_parser(b: &mut Builder) {
    // Integer.parseInt's Character.digit(char,10) domain is one UTF-16 unit,
    // including BMP decimal digits, excluding supplementary surrogate pairs.
    // The complete pinned JVM scan and ranges are retained with the evidence.
    let mut code = Vec::new();
    for start in DIGIT_STARTS {
        code.extend([
            LocalGet(0),
            I32Const(start),
            I32Sub,
            LocalTee(1),
            I32Const(10),
            I32LtU,
            If(BlockType::Empty),
            LocalGet(1),
            Return,
            End,
        ]);
    }
    code.push(I32Const(-1));
    let digit = b.function_with_locals(
        "decimal-context-digit",
        &[ValType::I32],
        &[ValType::I32],
        &[(1, ValType::I32)],
        &code,
    );

    // value0; length1, index2, negative3; magnitude4; unit5, digit6,
    // precision7, unused8, first-digit9. All indexing is length-guarded.
    let mut code = vec![
        LocalGet(0),
        RefCastNonNull(HeapType::Concrete(STRING)),
        ArrayLen,
        LocalSet(1),
        LocalGet(1),
        I32Const(27),
        I32LtU,
    ];
    bad_if(&mut code);
    code.extend(matches_text(None, "precision="));
    code.push(I32Eqz);
    bad_if(&mut code);
    code.extend([I32Const(10), LocalSet(2)]);
    code.extend(string_unit(vec![LocalGet(2)]));
    code.extend([
        LocalTee(5),
        I32Const(45),
        I32Eq,
        LocalSet(3),
        LocalGet(3),
        LocalGet(5),
        I32Const(43),
        I32Eq,
        I32Or,
        If(BlockType::Empty),
        I32Const(11),
        LocalSet(2),
        End,
        LocalGet(2),
        LocalSet(9),
        Block(BlockType::Empty),
        Loop(BlockType::Empty),
        LocalGet(2),
        LocalGet(1),
        I32GeU,
    ]);
    bad_if(&mut code);
    code.extend(string_unit(vec![LocalGet(2)]));
    code.extend([
        LocalTee(5),
        I32Const(32),
        I32Eq,
        BrIf(1),
        LocalGet(5),
        Call(digit),
        LocalTee(6),
        I32Const(0),
        I32LtS,
    ]);
    bad_if(&mut code);
    code.extend([
        LocalGet(4),
        I64Const(10),
        I64Mul,
        LocalGet(6),
        I64ExtendI32U,
        I64Add,
        LocalTee(4),
        I64Const(2147483647),
        LocalGet(3),
        I64ExtendI32U,
        I64Add,
        I64GtU,
    ]);
    bad_if(&mut code);
    code.extend([
        LocalGet(2),
        I32Const(1),
        I32Add,
        LocalSet(2),
        Br(0),
        End,
        End,
        LocalGet(2),
        LocalGet(9),
        I32Eq,
    ]);
    bad_if(&mut code);
    code.extend([
        LocalGet(2),
        I32Const(1),
        I32Add,
        LocalSet(2),
        LocalGet(1),
        LocalGet(2),
        I32Sub,
        I32Const(13),
        I32LtU,
    ]);
    bad_if(&mut code);
    code.extend(matches_text(Some(2), "roundingMode="));
    code.push(I32Eqz);
    bad_if(&mut code);
    code.extend([
        LocalGet(2),
        I32Const(13),
        I32Add,
        LocalSet(2),
        I64Const(0),
        LocalGet(4),
        I64Sub,
        LocalGet(4),
        LocalGet(3),
        Select,
        I32WrapI64,
        LocalSet(7),
    ]);
    for (mode, name) in MODES.iter().enumerate() {
        code.extend([
            LocalGet(1),
            LocalGet(2),
            I32Sub,
            I32Const(name.len() as i32),
            I32Eq,
            If(BlockType::Empty),
        ]);
        code.extend(matches_text(Some(2), name));
        code.extend([
            If(BlockType::Empty),
            LocalGet(7),
            I32Const(mode as i32),
            Return,
            End,
            End,
        ]);
    }
    code.extend(error("bad string format"));
    code.extend([I32Const(0), I32Const(0)]); // unreachable after the typed throw
    b.function_with_locals(
        "decimal-context-parse",
        &[VALUE],
        &[ValType::I32, ValType::I32],
        &[(3, ValType::I32), (1, ValType::I64), (5, ValType::I32)],
        &code,
    );
}
fn class_cast(class: &str) -> Vec<Instruction<'static>> {
    let message = if class.starts_with("java.") {
        format!(
            "class {class} cannot be cast to class java.lang.Number ({class} and java.lang.Number are in module java.base of loader 'bootstrap')"
        )
    } else {
        format!(
            "class {class} cannot be cast to class java.lang.Number ({class} is in unnamed module of loader 'app'; java.lang.Number is in module java.base of loader 'bootstrap')"
        )
    };
    error(&message)
}
fn nonnumber_errors(code: &mut Vec<Instruction<'static>>) {
    code.extend([LocalGet(0), RefIsNull, If(BlockType::Empty)]);
    code.extend(error(
        "Cannot invoke \"java.lang.Character.charValue()\" because \"x\" is null",
    ));
    code.push(End);
    code.extend([
        LocalGet(0),
        RefTestNonNull(HeapType::Concrete(STRING)),
        If(BlockType::Empty),
    ]);
    code.extend(class_cast("java.lang.String"));
    code.push(End);
    code.extend([
        LocalGet(0),
        GlobalGet(FALSE_GLOBAL),
        RefEq,
        LocalGet(0),
        GlobalGet(TRUE_GLOBAL),
        RefEq,
        I32Or,
        If(BlockType::Empty),
    ]);
    code.extend(class_cast("java.lang.Boolean"));
    code.push(End);
    code.extend([
        LocalGet(0),
        RefTestNonNull(HeapType::Concrete(OBJECT)),
        If(BlockType::Empty),
    ]);
    // Closed canonical collection descriptors identify their actual classes;
    // a lookalike user descriptor cannot acquire these primitive diagnostics.
    for (root, class) in [
        (KEYWORD_DESCRIPTOR_GLOBAL, "clojure.lang.Keyword"),
        (VECTOR_DESCRIPTOR_GLOBAL, "clojure.lang.PersistentVector"),
        (
            ARRAY_MAP_DESCRIPTOR_GLOBAL,
            "clojure.lang.PersistentArrayMap",
        ),
        (HASH_MAP_DESCRIPTOR_GLOBAL, "clojure.lang.PersistentHashMap"),
        (HASH_SET_DESCRIPTOR_GLOBAL, "clojure.lang.PersistentHashSet"),
    ] {
        code.extend([
            LocalGet(0),
            RefCastNonNull(HeapType::Concrete(OBJECT)),
            StructGet {
                struct_type_index: OBJECT,
                field_index: 0,
            },
            GlobalGet(root),
            RefEq,
            If(BlockType::Empty),
        ]);
        code.extend(class_cast(class));
        code.push(End);
    }
    code.push(End);
}
pub(super) fn function(b: &mut Builder) {
    text_parser(b);
    // precision0 and optional rounding1; checked precision2 and mode3.
    let mut code = vec![LocalGet(1), RefIsNull, If(BlockType::Empty)];
    code.extend([LocalGet(0), RefIsNull, If(BlockType::Empty)]);
    code.extend(error("null String"));
    code.push(End);
    code.extend([
        LocalGet(0),
        RefTestNonNull(HeapType::Concrete(STRING)),
        If(BlockType::Empty),
        LocalGet(0),
        Call(b.names["decimal-context-parse"]),
        LocalSet(3),
        LocalSet(2),
        Else,
        LocalGet(0),
        Call(b.names["long-test"]),
        I32Eqz,
        If(BlockType::Empty),
    ]);
    code.extend(error(
        "No matching ctor found for class java.math.MathContext",
    ));
    code.extend([
        End,
        LocalGet(0),
        Call(b.names["number-cast-int-raw"]),
        I32WrapI64,
        LocalSet(2),
        I32Const(4),
        LocalSet(3),
        End,
        Else,
    ]);
    nonnumber_errors(&mut code);
    code.extend([
        LocalGet(0),
        Call(b.names["number-cast-long-raw"]),
        LocalTee(4),
        I64Const(i64::from(i32::MIN)),
        I64LtS,
        LocalGet(4),
        I64Const(i64::from(i32::MAX)),
        I64GtS,
        I32Or,
        If(BlockType::Empty),
    ]);
    code.extend(error("integer overflow"));
    code.extend([
        End,
        LocalGet(4),
        I32WrapI64,
        LocalSet(2),
        LocalGet(1),
        Call(b.names["long-test"]),
        I32Eqz,
        If(BlockType::Empty),
    ]);
    code.extend(error("Invalid rounding mode"));
    code.extend([
        End,
        LocalGet(1),
        Call(b.names["long-unbox"]),
        LocalTee(4),
        I64Const(7),
        I64GtU,
        If(BlockType::Empty),
    ]);
    code.extend(error("Invalid rounding mode"));
    code.extend([
        End,
        LocalGet(4),
        I32WrapI64,
        LocalSet(3),
        End,
        LocalGet(2),
        I32Const(0),
        I32LtS,
        If(BlockType::Empty),
    ]);
    code.extend(error("Digits < 0"));
    code.extend([End, LocalGet(2), LocalGet(3), StructNew(MATH_CONTEXT)]);
    b.function_with_locals(
        "decimal-context-new",
        &[VALUE, VALUE],
        &[VALUE],
        &[(2, ValType::I32), (1, ValType::I64)],
        &code,
    );
}
