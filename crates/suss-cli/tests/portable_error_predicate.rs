use suss_cli::portable_session::Session;

#[test]
fn native_error_catch_adapter_distinguishes_errors_and_rethrows_other_payloads() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        for source in [
            "(suss.bootstrap/error? (suss.bootstrap/error \"message\"))",
            "(suss.bootstrap/error? (ex-info \"message\" {}))",
            "(not (suss.bootstrap/error? nil))",
            "(not (suss.bootstrap/error? 17))",
            "(not (suss.bootstrap/error? {}))",
            "(not (suss.bootstrap/error? (fn [] nil)))",
            "(do (def caught 0) (try (throw (suss.bootstrap/error \"x\")) (catch :default ex (if (suss.bootstrap/error? ex) (set! caught 42) (throw ex)))) (== caught 42))",
            "(do (def thrown {}) (try (try (throw thrown) (catch :default ex (if (suss.bootstrap/error? ex) false (throw ex)))) (catch :default ex (identical? ex thrown))))",
            "(do (def f (fn [x] x)) (try (f) (catch :default ex (suss.bootstrap/error? ex))))",
        ] {
            let value = session.eval(source).unwrap_or_else(|error| panic!("{source}: {error}"));
            session.collect().unwrap();
            assert_eq!(session.inspect(&value, |store, value| Ok(value.unwrap_anyref().unwrap().as_i31(&store)?.unwrap().get_u32())).unwrap(), 4, "{source}");
        }
        assert!(session.eval("(suss.bootstrap/error?)").is_err());
        assert!(session.eval("(suss.bootstrap/error? nil nil)").is_err());
    }
}
