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

#[test]
fn direct_linked_calls_use_raw_arguments_and_results() {
    use portable::resolve::{Environment, Phase};
    let mut environment = Environment::default();
    environment.set_direct_linking(true);
    let definition = portable::prepare_fragment("(def f (fn ^long [^long x] (* x 2)))", &environment, Phase::Runtime).unwrap();
    let bytes = portable::compile_in("(fn ^long [^long n] (* (f n) 7))", &definition.environment, Phase::Runtime).unwrap();
    let mut imports = Vec::new();
    let mut found = 0;
    for payload in Parser::new(0).parse_all(&bytes) {
        match payload.unwrap() {
            Payload::ImportSection(section) => for import in section.into_imports() {
                let import = import.unwrap();
                if let TypeRef::Func(_) = import.ty { imports.push(import.name.to_owned()); }
            },
            Payload::CodeSectionEntry(body) => {
                let ops = body.get_operators_reader().unwrap().into_iter().map(Result::unwrap).collect::<Vec<_>>();
                for (i, op) in ops.iter().enumerate() {
                    let Operator::Call { function_index } = op else { continue; };
                    if !imports.get(*function_index as usize).is_some_and(|name| name.ends_with(".typed")) { continue; }
                    found += 1;
                    // The successful guarded arm passes raw locals and saves
                    // the raw result. Only the boxed fallback can allocate.
                    let start = ops[..i].iter().rposition(|op| matches!(op, Operator::If { .. })).unwrap();
                    assert!(ops[start + 1..i].iter().all(|op| matches!(op, Operator::LocalGet { .. })), "{ops:?}");
                    assert!(matches!(ops.get(i + 1), Some(Operator::LocalSet { .. })), "{ops:?}");
                    let end = (i + 1..ops.len()).find(|j| matches!(ops[*j], Operator::Br { .. })).unwrap();
                    assert!(!ops[i + 1..end].iter().any(|op| matches!(op,
                        Operator::StructNew { .. } | Operator::RefI31 | Operator::ArrayNewFixed { .. })), "{ops:?}");
                }
            }
            _ => {}
        }
    }
    assert_eq!(found, 1, "the linked caller imports and calls its typed entry");
}

#[test]
fn live_var_calls_cache_and_invoke_the_primitive_signature() {
    use portable::resolve::{Environment, Phase};
    let definition = portable::prepare_fragment(
        "(def ^{:arglists '(^long [^long x])} f (fn ^long [^long x] (* x 2)))",
        &Environment::default(), Phase::Runtime).unwrap();
    let bytes = portable::compile_in("(fn [^long n] (f n))", &definition.environment, Phase::Runtime).unwrap();
    let mut types = Vec::new();
    let mut found = 0;
    for payload in Parser::new(0).parse_all(&bytes) {
        match payload.unwrap() {
            Payload::TypeSection(section) => for group in section {
                for subtype in group.unwrap().into_types() {
                    types.push(match subtype.composite_type.inner {
                        CompositeInnerType::Func(ty) => Some(ty), _ => None,
                    });
                }
            },
            Payload::CodeSectionEntry(body) => {
                let ops = body.get_operators_reader().unwrap().into_iter().map(Result::unwrap).collect::<Vec<_>>();
                for (i, op) in ops.iter().enumerate() {
                    let Operator::CallRef { type_index } = op else { continue; };
                    let ty = types[*type_index as usize].as_ref().unwrap();
                    if ty.params() != [ValType::Ref(wasmparser::RefType::EQREF), ValType::I64] || ty.results() != [ValType::I64] { continue; }
                    found += 1;
                    assert!(matches!(ops.get(i.wrapping_sub(1)), Some(Operator::RefAsNonNull))
                        && matches!(ops.get(i.wrapping_sub(2)), Some(Operator::GlobalGet { .. })),
                        "the hit calls a cached function reference: {ops:?}");
                    assert!(matches!(ops.get(i + 1), Some(Operator::LocalSet { .. })), "{ops:?}");
                }
            }
            _ => {}
        }
    }
    assert_eq!(found, 1, "live arglist hints select a guarded raw call");
}
