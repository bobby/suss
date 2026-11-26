#[path = "support/compilation_fuel.rs"]
mod compilation_fuel;

use suss_compile::{
    portable_macro_data::FormBridge, portable_macros::CompiledMacros, portable_session::Session,
};
use suss_reader::forms::{Kind, read_forms};
#[test]
fn actual_protocol_indexes_preserve_caller_phase_and_own_identity() {
    let mut failures = 0;
    for (phase, mut session) in [
        ("runtime", Session::new_repl().unwrap()),
        ("macro", Session::new_macro().unwrap()),
    ] {
        // Keep forged metadata as data while definitions remain executable.
        // Actual binding identity must still come from its real caller cell.
        session.enter_namespace("forged").unwrap();
        session.eval("(def Name 'forged/Name)").unwrap();
        session.enter_namespace("user").unwrap();
        let mut macros = CompiledMacros::new().unwrap();
        macros.set_operation_fuel(compilation_fuel::COMPILATION_FUEL);
        let full_source = r#"(defmacro ^{:suss/compiler-catalog true} protocol-index []
   (let [c (:suss/compiler-catalog &env)]
    (list 'quote [(get (get (:protocol-globals-by-namespace c) "suss.core") 'IHash)
                 (get (:unqualified-protocol-bindings c) 'IHash)
                 (get (:unqualified-protocol-bindings c) 'IMeta)])))"#;
        macros.define(full_source).unwrap();
        macros.define(r#"(defmacro ^{:suss/compiler-catalog :record-bindings} protocol-index-record []
          (let [resolve (fn [global]
                          (try (first (:protocols
                            (suss.internal.record-protocol-facts/resolve-record-compiler-facts &env [global])))
                            (catch :default error
                              (if (= (ex-message error) "Record protocol symbol is unresolved in the caller namespace")
                                nil (throw error)))))]
            (list 'quote [(resolve 'suss.core/IHash) (resolve 'IHash) (resolve 'IMeta)])))"#).unwrap();
        for (case, source, expected) in [
            (
                "own-metadata",
                "(ns record.projection) (def ^{:name forged/Name} IHash 17) (user/protocol-index)",
                "[suss.core/IHash record.projection/IHash suss.core/IMeta]",
            ),
            (
                "exclude",
                "(ns record.excluded (:refer-clojure :exclude [IMeta])) (user/protocol-index)",
                "[suss.core/IHash suss.core/IHash nil]",
            ),
            (
                "rename",
                "(ns record.renamed (:refer-clojure :rename {IMeta IHash})) (user/protocol-index)",
                "[suss.core/IHash suss.core/IMeta nil]",
            ),
        ] {
            for projection in ["protocol-index", "protocol-index-record"] {
                let source = source.replace("protocol-index", projection);
                let value = session.eval_with_macros(&source, &mut macros).unwrap();
                session.collect().unwrap();
                let bridge = FormBridge::new(&mut session).unwrap();
                let observed = bridge.read(&mut session, &value, 0..0).unwrap();
                let expected = read_forms(expected).unwrap().remove(0);
                println!("{phase}/{case}/{projection}: actual {:?}", observed.kind);
                let Kind::Vector(actual) = observed.into_parts().2 else {
                    panic!("fact vector")
                };
                let Kind::Vector(expected) = expected.into_parts().2 else {
                    panic!()
                };
                if actual
                    .iter()
                    .map(|x| &x.kind)
                    .ne(expected.iter().map(|x| &x.kind))
                {
                    failures += 1;
                }
            }
        }
    }
    assert_eq!(
        failures, 0,
        "real caller-phase catalog must expose signatures and actual own binding identity despite forged metadata"
    );
}
#[test]
fn compact_membership_uses_real_bindings_and_failed_staging_restores_mode() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        // Keep forged metadata as data while definitions remain executable.
        // Actual binding identity must still come from its real caller cell.
        session.enter_namespace("forged").unwrap();
        session.eval("(def Name 'forged/Name)").unwrap();
        session.enter_namespace("user").unwrap();
        let mut macros = CompiledMacros::new().unwrap();
        macros.set_operation_fuel(compilation_fuel::COMPILATION_FUEL);
        macros.define("(defmacro ^{:suss/compiler-catalog :record-bindings} membership [global] (let [view (:suss/catalog &env) facts (suss.internal.record-protocol-facts/resolve-record-compiler-facts &env [global])] (list 'quote [(:suss/projection &env) (contains? view :defs) (contains? view :suss/binding-names) (:protocols facts) (and (string? (first (:suss/binding-index view))) (vector? (second (:suss/binding-index view))))])))").unwrap();
        session
            .eval_with_macros(
                "(ns record.provider) (def ^{:name forged/Name} ordinary 17)",
                &mut macros,
            )
            .unwrap();
        let value = session
            .eval_with_macros(
                "(ns record.consumer) (user/membership record.provider/ordinary)",
                &mut macros,
            )
            .unwrap();
        session.collect().unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        let value = bridge.read(&mut session, &value, 0..0).unwrap();
        let Kind::Vector(parts) = value.into_parts().2 else {
            panic!("compact facts")
        };
        assert!(matches!(&parts[0].kind, Kind::Keyword(key) if key.name == "record-bindings"));
        assert!(
            matches!(&parts[1].kind, Kind::Bool(false)),
            "compact view must not manufacture defs"
        );
        assert!(
            matches!(&parts[2].kind, Kind::Bool(false)),
            "compact view must not allocate the legacy symbol set"
        );
        let Kind::Set(protocols) = &parts[3].kind else {
            panic!("actual membership result")
        };
        assert_eq!(protocols.len(), 1);
        assert!(matches!(&parts[4].kind, Kind::Bool(true)), "private membership index contains packed names and offsets");
        assert!(
            matches!(&protocols[0].kind, Kind::Symbol(symbol) if symbol.namespace.as_deref() == Some("record.provider") && symbol.name == "ordinary")
        );
        session.eval("(ns user)").unwrap();
        macros.define("(defmacro ^{:suss/compiler-catalog :record-bindings} mode [] (list 'quote (:suss/projection &env)))").unwrap();
        let failed = suss_cli::portable_repl::evaluate_script_compiled(
            &mut session,
            &mut macros,
            "(defmacro ^{:suss/compiler-catalog true} mode [] (list 'quote (:suss/projection &env))) compact-mode-unresolved",
            None,
        );
        assert!(failed.is_err());
        session.collect().unwrap();
        let value = session
            .eval_with_macros("(user/mode)", &mut macros)
            .unwrap();
        let value = bridge.read(&mut session, &value, 0..0).unwrap();
        assert!(matches!(&value.kind, Kind::Keyword(key) if key.name == "record-bindings"));
        suss_cli::portable_repl::evaluate_script_compiled(&mut session, &mut macros,
            "(defmacro ^{:suss/compiler-catalog true} mode [] (list 'quote (:suss/projection &env)))", None).unwrap();
        let value = session
            .eval_with_macros("(user/mode)", &mut macros)
            .unwrap();
        let value = bridge.read(&mut session, &value, 0..0).unwrap();
        assert!(
            matches!(&value.kind, Kind::Nil),
            "successful full-mode replacement must publish"
        );
    }
}

#[test]
fn membership_index_preserves_unicode_order_in_both_caller_phases() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut macros = CompiledMacros::new().unwrap();
        macros.set_operation_fuel(compilation_fuel::COMPILATION_FUEL);
        macros.define("(defmacro install-name [text] (list 'def (symbol text) 17))").unwrap();
        for mode in ["true", ":record-bindings"] {
            macros.define(&format!(r#"(defmacro ^{{:suss/compiler-catalog {mode}}} resolve-names [cases]
              (list 'quote
                (mapv (fn [entry]
                  (let [text (first entry) qualified (second entry)
                        global (if qualified (symbol "record.names" text) (symbol text))]
                    (try
                      (:protocols (suss.internal.record-protocol-facts/resolve-record-compiler-facts &env [global]))
                      (catch :default error [:error (ex-message error)])))) cases)))"#)).unwrap();
            session.eval_with_macros("(ns record.names)", &mut macros).unwrap();
            let names = ["\"a-b\"", "\"λ\"", "\"\u{10000}\"", "\"\u{e000}\""];
            for literal in names {
                session.eval_with_macros(&format!("(user/install-name {literal})"), &mut macros).unwrap();
            }
            // Materialize each full/private caller environment once. All lookups
            // still execute in compiled Suss against that exact phase/snapshot.
            let cases = names.into_iter().flat_map(|name| [(name, false), (name, true)])
                .chain([("\"absent\"", false), ("\"absent\"", true)]).collect::<Vec<_>>();
            let inputs = cases.iter().map(|(name, qualified)| format!("[{name} {qualified}]"))
                .collect::<Vec<_>>().join(" ");
            let value = session.eval_with_macros(&format!("(user/resolve-names [{inputs}])"), &mut macros).unwrap();
            session.collect().unwrap();
            let bridge = FormBridge::new(&mut session).unwrap();
            let actual = bridge.read(&mut session, &value, 0..0).unwrap();
            let Kind::Vector(results) = actual.into_parts().2 else { panic!("batch of real lookups") };
            assert_eq!(results.len(), cases.len());
            for ((literal, qualified), actual) in cases.into_iter().zip(results) {
                if literal != "\"absent\"" {
                    let expected = read_forms(literal).unwrap().remove(0);
                    let Kind::String(expected) = expected.into_parts().2 else { panic!("name string") };
                    let Kind::Set(protocols) = actual.into_parts().2 else { panic!("resolved identities") };
                    assert_eq!(protocols.len(), 1);
                    assert!(matches!(&protocols[0].kind, Kind::Symbol(symbol)
                        if symbol.namespace.as_deref() == Some("record.names")
                        && symbol.name.encode_utf16().eq(expected)), "{protocols:?}");
                } else {
                    let Kind::Vector(parts) = &actual.kind else { panic!("exact missing-binding error: {actual:?}") };
                    assert!(matches!(&parts[0].kind, Kind::Keyword(key) if key.name == "error"));
                    let Kind::String(message) = &parts[1].kind else { panic!("error message") };
                    let expected = if qualified {
                        "Record protocol symbol does not resolve to an actual caller-phase binding"
                    } else {
                        "Record protocol symbol is unresolved in the caller namespace"
                    };
                    assert_eq!(String::from_utf16(message).unwrap(), expected);
                }
            }
        }
    }
}
