//! Supplemental observation only: original root-mutation regression is preserved.
use suss_compile::portable_session::Session;

#[test]
fn primitive_string_root_writes_observe_caught_boundaries_in_both_phases() {
    let mut failures = Vec::new();
    for (phase, mut session) in [
        ("runtime", Session::new_repl().unwrap()),
        ("macro", Session::new_macro().unwrap()),
    ] {
        let observed = session.eval(r#"(def root-probe-events [])
          (def root-probe-method (.-charCodeAt "AB"))
          (def root-probe-replacement
            (fn [& ignored] (do (set! root-probe-events (conj root-probe-events 99)) -1)))
          (def root-probe-call-caught false)
          (def root-probe-apply-caught false)
          (def root-probe-selected
            (.charCodeAt "AB"
              (do
                (set! root-probe-call-caught
                  (try (do (set! (.-call root-probe-method) root-probe-replacement)
                           (set! root-probe-events (conj root-probe-events 11)) false)
                       (catch :default error
                         (do (set! root-probe-events (conj root-probe-events 10)) true))))
                (set! root-probe-apply-caught
                  (try (do (set! (.-apply root-probe-method) root-probe-replacement)
                           (set! root-probe-events (conj root-probe-events 21)) false)
                       (catch :default error
                         (do (set! root-probe-events (conj root-probe-events 20)) true))))
                (set! root-probe-events (conj root-probe-events 30))
                1)))
          (def root-probe-custom-caught
            (try (do (set! (.-auditRootField root-probe-method) 17)
                     (set! root-probe-events (conj root-probe-events 41)) false)
                 (catch :default error
                   (do (set! root-probe-events (conj root-probe-events 40)) true))))
          (def root-probe-ordinary (js-obj))
          (set! (.-auditRootField root-probe-ordinary) 17)
          [root-probe-selected (.call root-probe-method "AB" 1)
           root-probe-call-caught root-probe-apply-caught root-probe-custom-caught
           (.-auditRootField root-probe-ordinary) root-probe-events]"#);
        match observed {
            Ok(value) => match suss_compile::portable_repl::display(&mut session, &value) {
                Ok(row) => {
                    eprintln!("root-write-probe {phase}: {row}");
                    if row != "[66 66 true true true 17 [10 20 30 40]]" {
                        failures.push(format!("{phase}: unexpected observation {row}"));
                    }
                }
                Err(error) => failures.push(format!("{phase}: observation decode failed {error:?}")),
            },
            Err(error) => failures.push(format!("{phase}: execution failed {error:?}")),
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
