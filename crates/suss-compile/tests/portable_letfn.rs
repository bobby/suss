//! Recursive lexical groups lower to verified, executable closure graphs.
use suss_compile::portable::{self, hir::{Expression, LocalKind, SourceNode}, ir::{self}};
use wasmparser::{CompositeInnerType, Parser, Payload, StorageType, ValType};

#[test]
fn forward_references_and_nested_captures_verify_and_validate() {
    for source in [
        "(letfn [(f [n] (if (< n 1) 7 (g (- n 1)))) (g [n] (f n))] (f 3))",
        "(letfn* [f (fn [] g) g (fn [] f)] (f))",
        "(letfn [(f [] g) (g [] f)] (fn [] (f)))",
        "(fn [] (letfn [(f [] g) (g [] f)] (f)))",
        "(letfn [] 7)",
        "(letfn [(f [] 1) (f [] 2)] (f))",
    ] {
        let hir = portable::analyze(source).unwrap();
        let graph = ir::lower(&hir).unwrap();
        ir::verify(&graph).unwrap();
        portable::compile_ir(&graph).unwrap();
    }
}

#[test]
fn recursive_group_keeps_numeric_captures_in_raw_fields() {
    let bytes = portable::compile("(let [x 1.5] (letfn [(f [] (+ x 1.0)) (g [] (f))] g))").unwrap();
    let mut raw = 0;
    for payload in Parser::new(0).parse_all(&bytes) {
        if let Payload::TypeSection(section) = payload.unwrap() {
            for group in section {
                for ty in group.unwrap().types() {
                    if ty.supertype_idx.and_then(|index| index.as_module_index()) == Some(suss_compile::runtime_abi::CLOSURE)
                        && let CompositeInnerType::Struct(structure) = &ty.composite_type.inner
                    {
                        raw += structure.fields.iter().skip(3).filter(|field|
                            field.element_type == StorageType::Val(ValType::F64)).count();
                    }
                }
            }
        }
    }
    assert!(raw > 0, "recursive groups must not force numeric-only captures into boxes");
}

#[test]
fn source_bindings_describe_letfn_and_real_function_initializers() {
    let hir = portable::analyze("(letfn [(f [] g) (g [] f)] (f))").unwrap();
    let Expression::Do(forms) = &*hir.kind else { panic!("{hir:?}") };
    let node = forms.last().unwrap();
    let Some(SourceNode::Bindings { is_letfn: true, bindings, .. }) = node.source.as_ref().and_then(|source| source.node.as_deref()) else { panic!("{node:?}") };
    assert_eq!(bindings.len(), 2);
    for binding in bindings.iter() {
        assert_eq!(binding.kind, LocalKind::LetFn);
        assert!(binding.initializer.as_ref().unwrap().source.as_ref().unwrap().callable.is_some());
    }
}

#[test]
fn malformed_groups_are_located_diagnostics() {
    for source in ["(letfn)", "(letfn (f [] 1))", "(letfn [f] 1)",
        "(letfn* [f] f)", "(letfn* [f 1] f)", "(letfn [(ns/f [] 1)] 0)"]
    {
        let error = portable::analyze(source).unwrap_err();
        assert!(error.span.end <= source.len(), "{source}: {error}");
        assert!(error.message.contains("letfn"), "{source}: {error}");
    }
}
