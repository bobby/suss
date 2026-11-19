//! Primitive entries are real Wasm functions, owned by the code struct's
//! signature subtype. The generic entry is only a checked cast/call/box bridge.
use suss_compile::{portable, runtime_abi};
use wasmparser::{CompositeInnerType, FuncType, Operator, Parser, Payload, TypeRef, ValType};

fn check_entry(source: &str, parameters: &[ValType], result: ValType, error_calls: usize) {
    let bytes = portable::compile(source).unwrap();
    let mut types: Vec<Option<FuncType>> = Vec::new();
    let mut function_types = Vec::new();
    let mut imported = 0;
    let mut body_index = 0;
    let mut found = 0;
    let mut subtypes = 0;
    for payload in Parser::new(0).parse_all(&bytes) {
        match payload.unwrap() {
            Payload::TypeSection(section) => for group in section {
                for subtype in group.unwrap().into_types() {
                    subtypes += usize::from(subtype.supertype_idx.is_some_and(|index|
                        index.as_module_index() == Some(runtime_abi::CODE)));
                    types.push(match subtype.composite_type.inner {
                        CompositeInnerType::Func(ty) => Some(ty), _ => None,
                    });
                }
            },
            Payload::ImportSection(section) => for import in section.into_imports() {
                if let TypeRef::Func(ty) = import.unwrap().ty {
                    function_types.push(ty);
                    imported += 1;
                }
            },
            Payload::FunctionSection(section) => function_types.extend(section.into_iter().map(Result::unwrap)),
            Payload::CodeSectionEntry(body) => {
                let ty = types[function_types[imported + body_index] as usize].as_ref().unwrap();
                body_index += 1;
                if ty.params() != parameters || ty.results() != [result] { continue; }
                found += 1;
                let ops = body.get_operators_reader().unwrap().into_iter().map(Result::unwrap).collect::<Vec<_>>();
                let calls = ops.iter().enumerate().filter_map(|(i, op)|
                    matches!(op, Operator::Call { .. }).then_some(i)).collect::<Vec<_>>();
                assert_eq!(calls.len(), error_calls, "{ops:?}");
                for i in calls {
                    assert!(matches!(ops.get(i + 1), Some(Operator::Drop))
                        && matches!(ops.get(i + 2), Some(Operator::Unreachable)), "{ops:?}");
                }
                for (i, op) in ops.iter().enumerate() {
                    if matches!(op, Operator::StructNew { .. }) {
                        let end = (i..ops.len()).find(|j| matches!(ops[*j], Operator::Call { .. } | Operator::Return));
                        assert!(end.is_some_and(|j| matches!(ops[j], Operator::Call { .. })),
                            "a primitive entry boxes only on an error path: {ops:?}");
                    }
                    assert!(!matches!(op, Operator::ArrayNewFixed { .. } | Operator::ReturnCall { .. } | Operator::ReturnCallRef { .. }), "{ops:?}");
                }
            }
            _ => {}
        }
    }
    assert_eq!(found, 1, "missing unique primitive entry: {parameters:?} -> {result:?}");
    assert_eq!(subtypes, 1, "a primitive code struct extends the shared code type");
}

#[test]
fn primitive_entries_use_numeric_parameters_and_results_without_normal_boxes() {
    check_entry("(fn ^long [] 3000000000)", &[ValType::Ref(wasmparser::RefType::EQREF)], ValType::I64, 0);
    check_entry("(fn ^long [^long x] (* x 519))", &[ValType::Ref(wasmparser::RefType::EQREF), ValType::I64], ValType::I64, 1);
    check_entry("(fn ^double [^long a ^double b] (+ a b))",
        &[ValType::Ref(wasmparser::RefType::EQREF), ValType::I64, ValType::F64], ValType::F64, 0);
}
