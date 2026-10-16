#[test]
fn primitive_string_member_keeps_selected_body_when_index_mutates_root_properties() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let value = session.eval(r#"(def root-access-events [])
          (def root-access-method (.-charCodeAt "AB"))
          (def root-access-replacement
            (fn [& ignored] (do (set! root-access-events (conj root-access-events 99)) -1)))
          (let [value (.charCodeAt "AB"
                  (do (set! (.-call root-access-method) root-access-replacement)
                      (set! (.-apply root-access-method) root-access-replacement)
                      (set! root-access-events (conj root-access-events 1))
                      1))]
            [value (identical? root-access-replacement (.-call root-access-method))
                   (identical? root-access-replacement (.-apply root-access-method))
                   root-access-events])"#).unwrap_or_else(|error| match error {
            suss_compile::portable_session::SessionError::Language(value) => {
                let detail = suss_compile::portable_repl::display(&mut session, &value);
                panic!("root mutation language error: {detail:?}");
            }
            other => panic!("root mutation failed: {other:?}"),
        });
        let display = suss_compile::portable_repl::display(&mut session, &value).unwrap();
        assert_eq!(display, "[66 true true [1]]");
    }
}
