//! Actual Array conversion results and effects, not encoding-only evidence.
use suss_cli::portable_session::{Session, SessionError};
#[test]
fn array_join_conversion_preserves_holes_nesting_and_live_reads() {
    let cases = [
        "(= (.join (array 1 false nil (aget (array) 0) \"x\")) \"1,false,,,x\")",
        "(let [ctor (type (array)) a (ctor 3)] (aset a 1 \"x\") (= (.join a \"-\") \"-x-\"))",
        "(= (.join (array (array 1 2) (array 3))) \"1,2,3\")",
        "(let [a (array 1)] (aset a 1 a) (= (.join a) \"1,\"))",
        "(let [a (array \"a\") b (array \"b\")] (aset a 1 b) (aset b 1 a) (= (.join a) \"a,b,\"))",
        "(let [a (array nil 2) x (js-obj \"toString\" (fn [] (aset a 1 7) \"x\"))] (aset a 0 x) (= (.join a) \"x,7\"))",
        "(let [a (array 1 2) sep (js-obj \"toString\" (fn [] (aset a \"length\" 1) \"-\"))] (= (.join a sep) \"1-\"))",
        "(let [a (array nil 2) x (js-obj \"toString\" (fn [] (aset a \"length\" 1) \"x\"))] (aset a 0 x) (= (.join a) \"x,\"))",
        "(let [a (array 1 2)] (aset a \"join\" (fn [] \"overridden\")) (= (.toString a) \"overridden\"))",
        "(let [a (array 1)] (aset a \"join\" 7) (= (.toString a) \"[object Array]\"))",
        "(let [ctor (type 1)] (= (ctor (array 1)) 1))",
        "(let [ctor (type \"x\")] (= (ctor (array 1)) \"1\"))",
        "(let [ctor (type (array)) a (ctor 1000001)] (= (.join a \"\") \"\"))",
        "(let [ctor (type (array)) a (ctor 1000001)] (aset a 1000000 \"x\") (= (.join a \"\") \"x\"))",
        "(let [b (array (js-obj \"toString\" (fn [] (throw 17)))) a (array b)] (try (.join a) (catch :default e nil)) (aset b 0 \"fixed\") (= (.join a) \"fixed\"))",
        "(let [ctor (type (array)) a (ctor 1000001) x (js-obj \"toString\" (fn [] (aset a 1000000 \"later\") \"first\"))] (aset a 0 x) (= (.join a \"\") \"firstlater\"))",
        "(let [ctor (type (array)) a (ctor 1000001) x (js-obj \"toString\" (fn [] (aset a \"length\" 1) \"first\"))] (aset a 0 x) (aset a 1000000 \"removed\") (= (.join a \"\") \"first\"))",
        "(let [ctor (type (array)) a (ctor 2) x (js-obj \"toString\" (fn [] (aset a 1000000 \"outside\") \"first\"))] (aset a 0 x) (= (.join a \"\") \"first\"))",
    ];
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.set_operation_fuel(100_000_000);
        for source in cases {
            let value = session
                .eval(source)
                .unwrap_or_else(|error| panic!("{source}: {error}"));
            session.collect().unwrap();
            session
                .inspect(&value, |store, value| {
                    assert_eq!(
                        value
                            .unwrap_anyref()
                            .unwrap()
                            .as_i31(&store)?
                            .expect("Boolean ABI")
                            .get_u32(),
                        4,
                        "{source}"
                    );
                    Ok(())
                })
                .unwrap();
        }
    }
}
#[test]
fn array_join_cancellation_does_not_leave_a_cycle_marker() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.eval("(def join-entered (atom 0))").unwrap();
        let array = session.eval("(def retained-join-array (array (js-obj \"toString\" (fn [] (swap! join-entered inc) (loop [] (recur))))))").unwrap();
        let invoke = session.eval("(fn [a] (.join a))").unwrap();
        session.set_operation_fuel(1_000_000);
        let error = session.invoke(&invoke, &[&array]).unwrap_err();
        match error {
            SessionError::Trap(error) => assert_eq!(
                error.downcast_ref::<wasmtime::Trap>(),
                Some(&wasmtime::Trap::OutOfFuel)
            ),
            other => panic!("Expected runtime fuel trap: {other}"),
        }
        session.set_operation_fuel(100_000_000);
        let value = session.eval("(do (aset retained-join-array 0 \"fixed\") (and (= @join-entered 1) (= (.join retained-join-array) \"fixed\")))").unwrap();
        session.collect().unwrap();
        session
            .inspect(&value, |store, value| {
                assert_eq!(
                    value
                        .unwrap_anyref()
                        .unwrap()
                        .as_i31(&store)?
                        .unwrap()
                        .get_u32(),
                    4
                );
                Ok(())
            })
            .unwrap();
    }
}
