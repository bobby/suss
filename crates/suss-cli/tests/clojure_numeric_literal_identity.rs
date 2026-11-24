//! Actual exact-number constants, independent of primitive boxing/cache choices.
use suss_cli::{portable_macros::CompiledMacros, portable_session::Session};
use suss_compile::portable_session::sentinel_tag;

#[test]
fn exact_literal_function_sites_and_distinct_reads_follow_the_primary() {
    let cases: Vec<serde_json::Value> = serde_json::from_str(include_str!(
        "../../../results/m12-numeric-literal-identity/primary/cases.json")).unwrap();
    let names=["function-literal-bigint","separate-literal-bigint",
        "function-literal-wide-bigint","separate-literal-wide-bigint",
        "function-literal-ratio","separate-literal-ratio",
        "function-literal-decimal","separate-literal-decimal"];
    let mut failures=Vec::new();
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut macros=CompiledMacros::new().unwrap();
        for name in names {
            let row=cases.iter().find(|row|row["name"]==name).unwrap();
            let value=session.eval_with_macros(row["expression"].as_str().unwrap(),&mut macros).unwrap();
            session.collect().unwrap();
            let actual=session.inspect(&value,|mut store,value|sentinel_tag(&mut store,&value)).unwrap();
            let expected=if row["observed_edn"]=="true"{Some(4)}else{assert_eq!(row["observed_edn"],"false");Some(2)};
            if actual!=expected {failures.push(format!("{name}: expected {expected:?}, actual {actual:?}"));}
        }
    }
    assert!(failures.is_empty(),"{}",failures.join("\n"));
}

#[test]
fn exact_literal_return_roots_survive_calls_fragments_and_collection() {
    for mut session in [Session::new_repl().unwrap(),Session::new_macro().unwrap()] {
        let mut macros=CompiledMacros::new().unwrap();
        for token in ["1N","9223372036854775808N","1/3","1.0M"] {
            session.eval_with_macros(&format!("(def exact-literal-fn (fn [] {token}))"),&mut macros).unwrap();
            let first=session.eval_with_macros("(exact-literal-fn)",&mut macros).unwrap();
            let reference=session.inspect(&first,|mut store,value|value.unwrap_anyref().unwrap().to_owned_rooted(&mut store)).unwrap();
            session.collect().unwrap();
            let second=session.eval_with_macros("(exact-literal-fn)",&mut macros).unwrap();
            session.collect().unwrap();
            assert!(session.inspect(&second,|mut store,value|{
                let retained=reference.to_rooted(&mut store);
                wasmtime::Rooted::ref_eq(&store,value.unwrap_anyref().unwrap(),&retained)
            }).unwrap(),"{token}: one compiled literal must retain the actual object");
        }
    }
}
