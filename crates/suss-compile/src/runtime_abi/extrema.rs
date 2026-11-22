//! Original Clojure max/min scalar helpers, from fresh JVM 1.12.6 behavior.
//! Numbers.java and core.clj are EPL 1.0 references; no source is copied.
//! Object overloads propagate NaN before checking the other operand and return
//! the selected original value. Only double/double uses IEEE min/max zero rules.
use super::*;

pub(super) fn intrinsics(b: &mut Builder) {
    use Instruction::*;
    for (name, comparison, floating) in [
        ("max", "comparison-number-greater", F64Max),
        ("min", "comparison-number-less", F64Min),
    ] {
        let mut generic = Vec::new();
        for value in 0..2 {
            generic.extend([LocalGet(value), RefTestNonNull(HeapType::Concrete(NUMBER)), If(BlockType::Empty)]);
            for _ in 0..2 {
                generic.extend([LocalGet(value), RefCastNonNull(HeapType::Concrete(NUMBER)),
                    StructGet { struct_type_index: NUMBER, field_index: 0 }]);
            }
            generic.extend([F64Ne, If(BlockType::Empty), LocalGet(value), Return, End, End]);
        }
        numeric_operands::require_numbers(b, &mut generic, name, 2);
        generic.extend([LocalGet(0), LocalGet(1), LocalGet(0), LocalGet(1),
            Call(b.names[comparison]), sentinel_value(TRUE), RefEq, TypedSelect(VALUE)]);
        let generic = b.function(&format!("number-{name}"), &[VALUE, VALUE], &[VALUE], &generic);

        // Captures can be boxed and a primitive var can lose its signature.
        // Admit Math's source overload only for actual doubles; a replacement
        // value of another type keeps the generic operation without coercion.
        let mut primitive = vec![LocalGet(0), RefTestNonNull(HeapType::Concrete(NUMBER)),
            LocalGet(1), RefTestNonNull(HeapType::Concrete(NUMBER)), I32And, If(BlockType::Empty)];
        primitive.extend([LocalGet(0), Call(b.names["number-unbox"]),
            LocalGet(1), Call(b.names["number-unbox"]), floating, StructNew(NUMBER), Return, End,
            LocalGet(0), LocalGet(1), ReturnCall(generic)]);
        b.function(&format!("number-double-{name}"), &[VALUE, VALUE], &[VALUE], &primitive);
    }
}
