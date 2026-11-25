//! Original immutable decimal storage and scale-preserving conversion (#339).
use super::*;
use Instruction::*;
mod arithmetic;
mod addition;
mod context;
mod comparison;
mod conversions;

const DOUBLE_ROOT: u32 = MAP_ENTRY_DESCRIPTOR_GLOBAL + 2;
const CONTEXT_ROOT: u32 = DOUBLE_ROOT + 1;
const GETTER_ROOT: u32 = CONTEXT_ROOT + 1;

/// Decimal contagion applies before Ratio/BigInt, with Double still dominant.
pub(super) fn pair(b: &Builder) -> Vec<Instruction<'static>> {
    vec![LocalGet(0),Call(b.names["decimal-test"]),LocalGet(1),Call(b.names["decimal-test"]),I32Or,
        LocalGet(0),Call(b.names["double-test"]),LocalGet(1),Call(b.names["double-test"]),I32Or,I32Eqz,I32And]
}

pub(super) fn declare(b: &mut Builder) -> (u32, u32) {
    b.function("decimal-context-test", &[VALUE], &[ValType::I32], &[
        LocalGet(0), RefTestNonNull(HeapType::Concrete(MATH_CONTEXT)),
    ]);
    b.function("decimal-test", &[VALUE], &[ValType::I32], &[
        LocalGet(0), RefTestNonNull(HeapType::Concrete(DECIMAL)),
    ]);
    b.function("decimal-literal", &[VALUE, ValType::I32], &[VALUE], &[
        LocalGet(0), RefCastNonNull(HeapType::Concrete(BIGINT)), LocalGet(1), StructNew(DECIMAL),
    ]);
    let ty = b.next_type;
    b.types.ty().function([VALUE], [ValType::F64]);
    b.next_type += 1;
    b.function("decimal-double", &[VALUE], &[ValType::F64], &[
        LocalGet(0), GlobalGet(DOUBLE_ROOT), CallRef(ty),
    ]);
    let context_ty=b.next_type;
    b.types.ty().function([], [ValType::I32,ValType::I32]);
    b.next_type+=1;
    b.function("decimal-context", &[], &[ValType::I32,ValType::I32], &[
        GlobalGet(CONTEXT_ROOT),CallRef(context_ty),
    ]);
    context::registration(b);
    (ty,context_ty)
}

pub(super) fn append_global(globals: &mut GlobalSection, ty: u32, function: u32,
    context_ty: u32, context_function: u32) {
    assert_eq!(globals.len(), DOUBLE_ROOT);
    globals.global(GlobalType { val_type: reference(ty), mutable: false, shared: false },
        &ConstExpr::extended([RefFunc(function)]));
    assert_eq!(globals.len(),CONTEXT_ROOT);
    globals.global(GlobalType {val_type:reference(context_ty),mutable:false,shared:false},
        &ConstExpr::extended([RefFunc(context_function)]));
    assert_eq!(globals.len(),GETTER_ROOT);
    globals.global(GlobalType {val_type:VALUE,mutable:true,shared:false},
        &ConstExpr::ref_null(HeapType::Abstract {shared:false,ty:AbstractHeapType::Eq}));
}

/// Register after the checked integer casts used by the private reader constructor.
pub(super) fn constructor(b: &mut Builder) {
    let mut make = Vec::new();
    for local in [0, 1] {
        make.extend([LocalGet(local), Call(b.names["long-test"]),
            LocalGet(local), RefTestNonNull(HeapType::Concrete(BIGINT)), I32Or,
            I32Eqz, If(BlockType::Empty)]);
        make.extend(numeric::error("Decimal fields require exact integers", nominal::ERROR_GLOBAL));
        make.push(End);
    }
    make.extend([LocalGet(1), Call(b.names["number-cast-long-raw"]), LocalTee(2),
        I64Const(i64::from(i32::MIN)), I64LtS,
        LocalGet(2), I64Const(i64::from(i32::MAX)), I64GtS, I32Or,
        If(BlockType::Empty)]);
    make.extend(numeric::error("Decimal scale out of range", nominal::ERROR_GLOBAL));
    make.extend([End, LocalGet(0), Call(b.names["integer-bigint"]),
        RefCastNonNull(HeapType::Concrete(BIGINT)), LocalGet(2), I32WrapI64, StructNew(DECIMAL)]);
    b.function_with_locals("decimal-from-integer", &[VALUE, VALUE], &[VALUE], &[(1, ValType::I64)], &make);
}
pub(super) fn context_constructor(b: &mut Builder) { context::constructor(b); }
pub(super) fn late_functions(b: &mut Builder) { context::reader(b); }

pub(super) fn functions(b: &mut Builder, helper: numeric::Info) -> u32 {
    let reserve = b.names["numeric-reserve"];
    let byte = MemArg { offset: 0, align: 0, memory_index: 0 };
    let code = [
        LocalGet(0), RefCastNonNull(HeapType::Concrete(DECIMAL)),
        StructGet { struct_type_index: DECIMAL, field_index: 0 },
        Call(b.names["integer-biginteger"]), Call(b.names["bigint-string"]), LocalSet(1),
        LocalGet(1), RefCastNonNull(HeapType::Concrete(STRING)), ArrayLen, LocalTee(2),
        I64ExtendI32U, I64Const(2), I64Mul, I64Const(32), I64Add, Call(reserve), LocalSet(3),
        Block(BlockType::Empty), Loop(BlockType::Empty),
        LocalGet(7), LocalGet(2), I32GeU, BrIf(1),
        LocalGet(3), LocalGet(7), I32Add,
        LocalGet(1), RefCastNonNull(HeapType::Concrete(STRING)), LocalGet(7), ArrayGetU(STRING), I32Store8(byte),
        LocalGet(7), I32Const(1), I32Add, LocalSet(7), Br(0), End, End,
        LocalGet(3), LocalGet(2), I32Add, LocalSet(4),
        I32Const(helper.stack_top), GlobalSet(helper.stack_global),
        LocalGet(3), LocalGet(2), LocalGet(0), RefCastNonNull(HeapType::Concrete(DECIMAL)),
        StructGet { struct_type_index: DECIMAL, field_index: 1 }, LocalGet(4),
        Call(helper.decimal_format), LocalTee(5), ArrayNewDefault(STRING), LocalSet(6),
        I32Const(0), LocalSet(7),
        Block(BlockType::Empty), Loop(BlockType::Empty),
        LocalGet(7), LocalGet(5), I32GeU, BrIf(1),
        LocalGet(6), RefCastNonNull(HeapType::Concrete(STRING)), LocalGet(7),
        LocalGet(4), LocalGet(7), I32Add, I32Load8U(byte), ArraySet(STRING),
        LocalGet(7), I32Const(1), I32Add, LocalSet(7), Br(0), End, End,
        LocalGet(6),
    ];
    b.function_with_locals("decimal-string", &[VALUE], &[VALUE], &[
        (1, VALUE), (4, ValType::I32), (1, VALUE), (1, ValType::I32),
    ], &code);

    // Convert exact canonical text with the requested single rounding stage.
    let mut double = 0;
    for (name, parse, integer) in [
        ("decimal-double-impl", helper.parse, false),
        ("decimal-float", helper.parse_float, false),
        ("integer-float", helper.parse_float, true),
    ] {
        let mut code = vec![LocalGet(0)];
        if integer {
            code.extend([Call(b.names["integer-biginteger"]), Call(b.names["bigint-string"])]);
        } else { code.push(Call(b.names["decimal-string"])); }
        code.extend([
            LocalSet(1),
            LocalGet(1), RefCastNonNull(HeapType::Concrete(STRING)), ArrayLen, LocalTee(2),
            I64ExtendI32U, I64Const(2), I64Mul, Call(reserve), LocalSet(3),
            Block(BlockType::Empty), Loop(BlockType::Empty),
            LocalGet(4), LocalGet(2), I32GeU, BrIf(1),
            LocalGet(3), LocalGet(4), I32Const(1), I32Shl, I32Add,
            LocalGet(1), RefCastNonNull(HeapType::Concrete(STRING)), LocalGet(4), ArrayGetU(STRING), I32Store16(byte),
            LocalGet(4), I32Const(1), I32Add, LocalSet(4), Br(0), End, End,
            I32Const(helper.stack_top), GlobalSet(helper.stack_global),
            LocalGet(3), LocalGet(2), Call(parse),
        ]);
        let index = b.function_with_locals(name, &[VALUE], &[ValType::F64], &[
            (1, VALUE), (3, ValType::I32),
        ], &code);
        if name == "decimal-double-impl" { double = index; }
    }
    context::string(b);
    arithmetic::functions(b);
    comparison::functions(b);
    conversions::functions(b);
    double
}
