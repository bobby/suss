//! Execute nested associative core helpers against pinned JVM observations.
use suss_cli::portable_session::Session;
fn printed(session: &mut Session, source: &str) -> String {
    let value = session.eval(source).unwrap();
    session.collect().unwrap();
    session
        .inspect(&value, |mut store, value| {
            let string = value.unwrap_anyref().unwrap().as_array(&store)?.unwrap();
            let units = string
                .elems(&mut store)?
                .map(|v| v.unwrap_i32() as u16)
                .collect::<Vec<_>>();
            Ok(String::from_utf16(&units).unwrap())
        })
        .unwrap()
}
#[test]
fn get_in_and_assoc_in_match_primary_nested_paths_in_both_compiled_phases() {
    let cases: Vec<(String, String)> = serde_json::from_str(include_str!(
        "../../../results/m5-nested-associative/primary/cases.json"
    ))
    .unwrap();
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        for (source, expected) in &cases {
            assert_eq!(
                printed(&mut session, &format!("(pr-str {source})")),
                *expected,
                "{source}"
            );
        }
    }
}

#[test]
fn ordinary_source_core_loads_and_reads_the_live_get_callback_in_both_phases() {
    use suss_cli::portable_session::SessionOptions;
    use suss_compile::portable::resolve::Phase;
    for phase in [Phase::Runtime, Phase::Macro] {
        let mut session = Session::with_options_in(SessionOptions::default(), phase).unwrap();
        session.set_operation_fuel(100_000_000);
        session
            .eval(include_str!("../../../runtime/core-import/suss/core.sus"))
            .unwrap();
        session.enter_namespace("user").unwrap();
        assert_eq!(
            printed(
                &mut session,
                r#"(pr-str
          [(get-in {:a {:b 7}} [:a :b])
           (get-in {:a false} [:a] :missing)
           (get-in {} [:a :b] :missing)
           (assoc-in {} [:a :b] 9)
           (let [saved suss.core/get]
             (try
               (set! suss.core/get (fn [m k] [:shadow k]))
               [(get-in {} [:a :b]) (get-in {:a 11} [:a] :missing)]
               (finally (set! suss.core/get saved))))])"#
            ),
            "[7 false :missing {:a {:b 9}} [[:shadow :b] 11]]"
        );
        let error = session
            .eval("(suss.bootstrap/public-core-value suss.core/get)")
            .err()
            .expect("private core adapter must reject an ordinary caller")
            .to_string();
        assert!(error.contains("requires compiled core"), "{error}");
        session.enter_namespace("suss.core").unwrap();
        for source in [
            "(suss.bootstrap/public-core-value)",
            "(suss.bootstrap/public-core-value get get)",
            "(suss.bootstrap/public-core-value 1)",
            "(suss.bootstrap/public-core-value make-binding-var)",
            "(suss.bootstrap/public-core-value user/outside)",
        ] {
            assert!(session.eval(source).is_err(), "{source}");
        }
    }
}
