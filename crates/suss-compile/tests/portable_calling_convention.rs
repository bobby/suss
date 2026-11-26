//! The compiled form of ADR-0004's calling convention (#353): call sites with
//! no argument array or runtime `invoke`, static calls of known callees, and
//! runtime ABI 7 artifacts.
use suss_compile::portable::{
    self,
    resolve::{Environment, Phase},
};
use suss_compile::runtime_abi;
use wasmparser::{Parser, Payload, TypeRef};

/// Per defined function: its operators, as debug strings; and the names of
/// the fragment's function imports, by index.
fn functions(source: &str) -> (Vec<Vec<String>>, Vec<String>) {
    let wasm = portable::compile_in(source, &Environment::default(), Phase::Runtime).unwrap();
    let (mut bodies, mut imports) = (vec![], vec![]);
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
            Payload::CodeSectionEntry(body) => bodies.push(
                body.get_operators_reader()
                    .unwrap()
                    .into_iter()
                    .map(|operator| format!("{:?}", operator.unwrap()))
                    .collect(),
            ),
            _ => {}
        }
    }
    (bodies, imports)
}
fn calls(body: &[String], imports: &[String]) -> Vec<String> {
    body.iter()
        .filter_map(|operator| {
            let index: usize = operator
                .strip_prefix("Call { function_index: ")
                .or_else(|| operator.strip_prefix("ReturnCall { function_index: "))?
                .trim_end_matches(" }")
                .parse()
                .unwrap();
            Some(
                imports
                    .get(index)
                    .cloned()
                    .unwrap_or_else(|| "defined".into()),
            )
        })
        .collect()
}

#[test]
fn call_sites_call_the_entry_without_an_argument_array() {
    let (bodies, imports) = functions("(fn [g x] (g x))");
    // The method of `(fn [g x] …)`, which calls an unknown closure.
    let method = &bodies[1];
    // The site calls the entry it cached in a global for its last callee
    // (ABI 6, #378), with no cast to `$closure` and no read of a function
    // reference from a struct.
    assert!(
        !method.iter().any(|op| op.starts_with("StructGet")),
        "{method:?}"
    );
    assert!(
        !method
            .iter()
            .any(|op| op.starts_with("BrOnCastFail") || op.starts_with("RefCast")),
        "{method:?}"
    );
    assert!(
        method
            .iter()
            .any(|op| *op == format!("ReturnCallRef {{ type_index: {} }}", runtime_abi::CALL + 1)),
        "{method:?}"
    );
    assert!(
        !method.iter().any(|op| op.starts_with("ArrayNewFixed")),
        "{method:?}"
    );
    // Its only call finds a new callee's entry, and no call site uses `invoke`.
    assert_eq!(calls(method, &imports), ["call-entry-1"]);
    assert!(!imports.iter().any(|name| name == "invoke"), "{imports:?}");
}

#[test]
fn known_callees_are_static_calls() {
    for source in [
        // Returning k makes its identity observable and retains a real call.
        "(fn [] (let [k (fn [x] x)] (do (k 1) k)))",
        "(fn self [n] (self n))",
    ] {
        let (bodies, imports) = functions(source);
        // The first method, after `eval`: it calls the known callee's entry.
        let caller = &bodies[1];
        assert!(
            calls(caller, &imports).iter().any(|call| call == "defined"),
            "{source}: {caller:?}"
        );
        assert!(
            !caller
                .iter()
                .any(|op| op.starts_with("CallRef") || op.starts_with("ReturnCallRef")),
            "{source}"
        );
        assert!(
            !caller.iter().any(|op| op.starts_with("BrOnCastFail")),
            "{source}"
        );
    }
}

#[test]
fn a_locally_called_closure_needs_no_call_entry() {
    let (bodies, imports) = functions("(fn [] (let [k (fn [x] x)] (k 1)))");
    let caller = &bodies[1];
    assert!(calls(caller, &imports).is_empty(), "{caller:?}");
    assert!(
        !caller
            .iter()
            .any(|op| op.starts_with("CallRef") || op.starts_with("ReturnCallRef")),
        "{caller:?}"
    );
}

#[test]
fn artifacts_record_runtime_abi_13_and_prior_abis_are_rejected() {
    // ABI 13 roots canonical Unbound values in binding cells.
    // ABI 12 adds immutable MathContext values; ABI 11 adds Decimals.
    // ABI 10 adds immutable Ratios.
    // ABI 7 (#448) makes CODE extensible for primitive signature subtypes;
    // ABI 6's eqref closure parameter and call-site caches remain.
    assert_eq!(runtime_abi::VERSION, 13);
    let module = |abi| {
        let mut module = wasm_encoder::Module::new();
        module
            .section(
                &runtime_abi::Manifest {
                    runtime_abi: abi,
                    ..Default::default()
                }
                .section(),
            )
            .section(&runtime_abi::prelude());
        module.finish()
    };
    let expected = runtime_abi::Manifest::default();
    assert_eq!(runtime_abi::verify_artifact(&module(13), &expected), Ok(()));
    for abi in [5, 6, 7, 8, 9, 10, 11, 12] {
        let error = runtime_abi::verify_artifact(&module(abi), &expected).unwrap_err();
        assert!(error.starts_with("runtime ABI version mismatch"), "{error}");
    }
}
