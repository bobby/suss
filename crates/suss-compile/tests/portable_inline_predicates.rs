//! Core's :inline predicates compile without a call (#356).
use suss_compile::portable::{
    self,
    resolve::{Environment, Phase},
};

/// The runtime functions a compiled expression imports.
fn imports(source: &str) -> Vec<String> {
    use wasmparser::{Parser, Payload};
    let wasm = portable::compile_in(source, &Environment::default(), Phase::Runtime).unwrap();
    let mut names = vec![];
    for payload in Parser::new(0).parse_all(&wasm) {
        if let Payload::ImportSection(section) = payload.unwrap() {
            for import in section.into_imports() {
                let import = import.unwrap();
                if import.module == "suss.runtime" {
                    names.push(import.name.to_owned());
                }
            }
        }
    }
    names
}

/// Whether a compiled expression calls a function (ADR-0004): through
/// `call_ref` or the runtime's out-of-line `call-k`, or by a static call of a
/// known callee's entry. Spread entries, which call their methods, don't count.
fn calls(source: &str) -> bool {
    use wasmparser::{Operator, Parser, Payload, TypeRef};
    let wasm = portable::compile_in(source, &Environment::default(), Phase::Runtime).unwrap();
    let (mut imports, mut types, mut found) = (vec![], vec![], false);
    let mut body = 0;
    for payload in Parser::new(0).parse_all(&wasm) {
        match payload.unwrap() {
            Payload::ImportSection(section) => {
                for import in section.into_imports() {
                    let import = import.unwrap();
                    if matches!(import.ty, TypeRef::Func(_)) {
                        imports.push(import.name.to_owned());
                    }
                }
            }
            Payload::FunctionSection(section) => types.extend(section.into_iter().map(Result::unwrap)),
            Payload::CodeSectionEntry(code) => {
                let spread = types[body] == suss_compile::runtime_abi::SPREAD;
                body += 1;
                if spread {
                    continue;
                }
                for operator in code.get_operators_reader().unwrap() {
                    found |= match operator.unwrap() {
                        Operator::CallRef { .. } | Operator::ReturnCallRef { .. } => true,
                        Operator::Call { function_index } | Operator::ReturnCall { function_index } => {
                            imports.get(function_index as usize).is_none_or(|name| name.starts_with("call-"))
                        }
                        _ => false,
                    };
                }
            }
            _ => {}
        }
    }
    found
}

#[test]
fn nil_and_identical_compile_inline_unless_shadowed_or_of_another_arity() {
    let call = calls;
    for source in [
        "(fn [x] (nil? x))",
        "(fn [x y] (identical? x y))",
        "(fn [x] (if (nil? x) 1 2))",
        "(fn [x] (cljs.core/identical? x nil))",
    ] {
        assert!(!call(source), "{source} calls");
    }
    // Clojure inlines only these two: other predicates stay live calls.
    for source in [
        "(fn [x] (let [nil? (fn [y] 1)] (nil? x)))",
        "(fn [x] (nil? x x))",
        "(fn [x] (true? x))",
        "(fn [x] ((fn [f] (f x x)) identical?))",
    ] {
        assert!(call(source), "{source} doesn't call");
    }
    // Only distinct boxed numbers or strings reach the runtime's test.
    assert!(imports("(fn [x y] (identical? x y))").iter().any(|name| name == "identical-value"));
}
