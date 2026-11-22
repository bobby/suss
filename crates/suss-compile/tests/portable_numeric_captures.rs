//! Capture layout follows complete numeric uses, without changing source types.
use suss_compile::portable;
use wasmparser::{CompositeInnerType, Parser, Payload, StorageType, ValType};

fn capture_fields(source: &str) -> Vec<StorageType> {
    let mut environment = portable::resolve::Environment::default();
    for name in ["=", "identical?"] {
        environment.declare_cell(portable::resolve::Phase::Runtime, "suss.core", name).unwrap();
    }
    let bytes = portable::compile_in(source, &environment, portable::resolve::Phase::Runtime).unwrap();
    let mut captures = Vec::new();
    for payload in Parser::new(0).parse_all(&bytes) {
        if let Payload::TypeSection(section) = payload.unwrap() {
            for group in section {
                for ty in group.unwrap().types() {
                    if ty.supertype_idx.and_then(|index| index.as_module_index()) == Some(suss_compile::runtime_abi::CLOSURE)
                        && let CompositeInnerType::Struct(structure) = &ty.composite_type.inner
                        && structure.fields.len() == 4
                    {
                        captures.push(structure.fields[3].element_type);
                    }
                }
            }
        }
    }
    captures
}

#[test]
fn numeric_only_captures_use_raw_fields_in_ordinary_named_and_general_closures() {
    for (source, expected) in [
        ("(let [x 3000000000] (fn [] (+ x 7)))", ValType::I64),
        ("(let [x -0.0] (fn [] (* x 2.0)))", ValType::F64),
        ("(let [x 3000000000] (fn again [n] (if (= n 0) (+ x 1) (again (- n 1)))))", ValType::I64),
        ("(let [x 3000000000] (fn ([] (+ x 1)) ([a] (+ x 2))))", ValType::I64),
        ("(let [x 3000000000] (fn ^long [] (+ x 1)))", ValType::I64),
        ("(let [x 3000000000] (fn [b] (+ (if b x x) 7)))", ValType::I64),
    ] {
        assert!(capture_fields(source).contains(&StorageType::Val(expected)), "{source}");
    }
}

#[test]
fn making_a_numeric_only_closure_does_not_allocate_a_capture_box() {
    use wasmparser::Operator;
    for source in [
        "(let [x 3000000000] (fn [] (+ x 7)))",
        "(let [x 1.5] (fn [] (* x 2.0)))",
    ] {
        let bytes = portable::compile(source).unwrap();
        let maker = Parser::new(0).parse_all(&bytes).find_map(|payload| match payload.unwrap() {
            Payload::CodeSectionEntry(body) => Some(body),
            _ => None,
        }).unwrap();
        let structures = maker.get_operators_reader().unwrap().into_iter()
            .map(Result::unwrap).filter(|op| matches!(op, Operator::StructNew { .. })).count();
        assert_eq!(structures, 1, "only the closure allocates at its maker: {source}");
    }
}

#[test]
fn capture_object_uses_and_join_aliases_keep_the_shared_box() {
    for (source, count) in [
        ("(let [x 3000000000] (fn [] x))", 1),
        ("(let [x 1.5] (fn [b] (if b x x)))", 1),
        ("(let [x 3000000000] (fn [] (identical? x x)))", 1),
        ("(let [x 3000000000] (fn [] (fn [] x)))", 2),
    ] {
        assert_eq!(capture_fields(source), vec![StorageType::Val(ValType::Ref(wasmparser::RefType::EQREF)); count], "{source}");
    }
}
