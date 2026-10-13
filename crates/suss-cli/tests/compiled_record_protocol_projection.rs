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
        let full_source = r#"(defmacro ^{:suss/compiler-catalog true} protocol-index []
   (let [c (:suss/compiler-catalog &env)]
    (list 'quote [(get (get (:protocol-globals-by-namespace c) "suss.core") 'IHash)
                 (get (:unqualified-protocol-bindings c) 'IHash)
                 (get (:unqualified-protocol-bindings c) 'IMeta)])))"#;
        macros.define(full_source).unwrap();
        macros
            .define(
                &full_source
                    .replace(
                        ":suss/compiler-catalog true",
                        ":suss/compiler-catalog :record-bindings",
                    )
                    .replace("protocol-index", "protocol-index-record"),
            )
            .unwrap();
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
                let Kind::Vector(actual) = observed.kind else {
                    panic!("fact vector")
                };
                let Kind::Vector(expected) = expected.kind else {
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
        macros.define("(defmacro ^{:suss/compiler-catalog :record-bindings} membership [global] (let [view (:suss/catalog &env) facts (suss.internal.record-protocol-facts/resolve-record-compiler-facts &env [global])] (list 'quote [(:suss/projection &env) (contains? view :defs) (contains? (:suss/binding-names view) 'ordinary) (:protocols facts)])))").unwrap();
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
        let Kind::Vector(parts) = value.kind else {
            panic!("compact facts")
        };
        assert!(matches!(&parts[0].kind, Kind::Keyword(key) if key.name == "record-bindings"));
        assert!(
            matches!(parts[1].kind, Kind::Bool(false)),
            "compact view must not manufacture defs"
        );
        assert!(
            matches!(parts[2].kind, Kind::Bool(false)),
            "provider binding is not an own binding"
        );
        let Kind::Set(protocols) = &parts[3].kind else {
            panic!("actual membership result")
        };
        assert_eq!(protocols.len(), 1);
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
        assert!(matches!(value.kind, Kind::Keyword(key) if key.name == "record-bindings"));
        suss_cli::portable_repl::evaluate_script_compiled(&mut session, &mut macros,
            "(defmacro ^{:suss/compiler-catalog true} mode [] (list 'quote (:suss/projection &env)))", None).unwrap();
        let value = session
            .eval_with_macros("(user/mode)", &mut macros)
            .unwrap();
        let value = bridge.read(&mut session, &value, 0..0).unwrap();
        assert!(
            matches!(value.kind, Kind::Nil),
            "successful full-mode replacement must publish"
        );
    }
}
