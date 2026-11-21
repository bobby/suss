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
                            (catch :default _ nil)))]
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
fn membership_index_preserves_unicode_order_and_names_with_delimiters() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut macros = CompiledMacros::new().unwrap();
        macros.set_operation_fuel(compilation_fuel::COMPILATION_FUEL);
        macros.define("(defmacro install-name [text] (list 'def (symbol text) 17))").unwrap();
        for mode in ["true", ":record-bindings"] {
            macros.define(&format!(r#"(defmacro ^{{:suss/compiler-catalog {mode}}} resolve-name [text qualified]
              (let [global (if qualified (symbol "record.names" text) (symbol text))
                    facts (suss.internal.record-protocol-facts/resolve-record-compiler-facts &env [global])]
                (list 'quote (:protocols facts))))"#)).unwrap();
            session.eval_with_macros("(ns record.names)", &mut macros).unwrap();
            for literal in ["\"a b\"", "\"a\\u0000b\"", "\"\u{10000}\"", "\"\u{e000}\""] {
                session.eval_with_macros(&format!("(user/install-name {literal})"), &mut macros).unwrap();
            }
            let bridge = FormBridge::new(&mut session).unwrap();
            for literal in ["\"a b\"", "\"a\\u0000b\"", "\"\u{10000}\"", "\"\u{e000}\""] {
                for qualified in [false, true] {
                    let value = session.eval_with_macros(&format!("(user/resolve-name {literal} {qualified})"), &mut macros).unwrap();
                    let actual = bridge.read(&mut session, &value, 0..0).unwrap();
                    let expected = read_forms(literal).unwrap().remove(0);
                    let Kind::String(expected) = expected.into_parts().2 else { panic!("name string") };
                    let Kind::Set(protocols) = actual.into_parts().2 else { panic!("resolved identities") };
                    assert_eq!(protocols.len(), 1);
                    assert!(matches!(&protocols[0].kind, Kind::Symbol(symbol)
                        if symbol.namespace.as_deref() == Some("record.names")
                        && symbol.name.encode_utf16().eq(expected)), "{protocols:?}");
                }
            }
            for qualified in [false, true] {
                assert!(session.eval_with_macros(&format!("(user/resolve-name \"absent\" {qualified})"), &mut macros).is_err());
            }
        }
    }
}
