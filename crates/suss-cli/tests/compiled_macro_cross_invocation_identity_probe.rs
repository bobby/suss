//! Executed cross-invocation identity against fresh pinned analyzer observations.
use suss_cli::{
    portable_macro_data::FormBridge, portable_macros::CompiledMacros, portable_session::Session,
};
use suss_reader::forms::Kind;

#[test]
fn same_captured_declaration_metadata_across_distinct_macro_invocations() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut macros = CompiledMacros::new().unwrap();
        let observer = r#"(defmacro observe [label binding]
          (let [init (get (get (get &env :locals) binding) :init)
                info (get init :info)
                metadata (get info :meta)]
            (defonce saved metadata)
            (defonce saved-info info)
            (defonce outer-identities [])
            (set! outer-identities (conj outer-identities (identical? saved-info info)))
            (list 'quote [label (identical? saved metadata)
                          (get metadata :doc)])))"#;
        macros.define(observer).unwrap();
        let value = session
            .eval_with_macros(
                r#"
          (def ^{:doc "before"} watched 17)
          (def initial
            [(let [copy watched] (observe "first" copy))
             (let [copy watched] (observe "same-revision" copy))])
          (def ^{:doc "after"} watched 17)
          (def changed
            [(let [copy watched] (observe "new-revision" copy))
             (let [copy watched] (observe "same-new-revision" copy))])
          [initial changed watched]"#,
                &mut macros,
            )
            .unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        session.collect().unwrap();
        let result = bridge.read(&mut session, &value, 0..1).unwrap();
        let Kind::Vector(groups) = result.kind else {
            panic!("probe groups")
        };
        assert_eq!(groups.len(), 3);
        for (index, (labels, expected_identity, doc)) in [
            (["first", "same-revision"], [true, true], "before"),
            (
                ["new-revision", "same-new-revision"],
                [false, false],
                "after",
            ),
        ]
        .into_iter()
        .enumerate()
        {
            let Kind::Vector(rows) = &groups[index].kind else {
                panic!("probe rows")
            };
            assert_eq!(rows.len(), 2);
            for (i, row) in rows.iter().enumerate() {
                let Kind::Vector(fields) = &row.kind else {
                    panic!("probe fields")
                };
                assert_eq!(fields.len(), 3);
                assert!(
                    matches!(&fields[0].kind, Kind::String(s) if *s == labels[i].encode_utf16().collect::<Vec<_>>())
                );
                assert!(
                    matches!(fields[1].kind, Kind::Bool(b) if b == expected_identity[i]),
                    "captured declaration identity: {}: {:?}",
                    labels[i],
                    fields[1]
                );
                assert!(
                    matches!(&fields[2].kind, Kind::String(s) if *s == doc.encode_utf16().collect::<Vec<_>>())
                );
            }
        }
        assert!(matches!(groups[2].kind, Kind::Number(n) if n == 17.0));
        macros
            .define(
                r#"(defmacro retained-projection []
            (list 'quote [outer-identities (get saved :doc)]))"#,
            )
            .unwrap();
        let retained = session
            .eval_with_macros("(retained-projection)", &mut macros)
            .unwrap();
        assert_eq!(
            suss_cli::portable_repl::display(&mut session, &retained).unwrap(),
            "[[true false false false] \"before\"]"
        );
        suss_cli::portable_repl::reset_compiled(&mut session, &mut macros).unwrap();
        macros.define(observer).unwrap();
        let fresh = session
            .eval_with_macros(
                r#"(def ^{:doc "before"} watched 17)
            [(let [copy watched] (observe "first" copy))
             (let [copy watched] (observe "same-revision" copy))]"#,
                &mut macros,
            )
            .unwrap();
        assert_eq!(
            suss_cli::portable_repl::display(&mut session, &fresh).unwrap(),
            r#"[["first" true "before"] ["same-revision" true "before"]]"#
        );
    }
}
