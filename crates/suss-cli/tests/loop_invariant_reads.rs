//! A loop reads a var once when nothing in it can change the var (#389):
//! no call, var write or dynamic scope. Any of those keeps the read in
//! every iteration, so a loop sees a var's new value.
use suss_cli::portable_session::Session;

fn printed(session: &mut Session, source: &str) -> String {
    let value = session
        .eval(&format!("(pr-str {source})"))
        .unwrap_or_else(|error| panic!("{source}: {error}"));
    let units = session
        .inspect(&value, |mut store, value| {
            let array = value.unwrap_anyref().unwrap().as_array(&store)?.unwrap();
            Ok(array.elems(&mut store)?.map(|unit| unit.unwrap_i32() as u16).collect::<Vec<_>>())
        })
        .unwrap();
    String::from_utf16(&units).unwrap()
}

#[test]
fn loops_read_vars_once_only_when_nothing_can_change_them() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session
            .eval(
                "(def x 2)
                 (def a (long-array [1 2 3 4]))
                 (def sum-x (fn [n] (loop [i 0 acc 0] (if (< i n) (recur (inc i) (+ acc x)) acc))))
                 (def sum-a (fn [] (loop [i 0 acc 0] (if (< i 4) (recur (inc i) (+ acc (aget a i))) acc))))
                 (def y 1)
                 (def bump (fn [] (set! y (+ y 1))))
                 (def sum-y (fn [] (loop [i 0 acc 0] (if (< i 3) (do (bump) (recur (inc i) (+ acc y))) acc))))
                 (def ^:dynamic *d* 1)
                 (def sum-d (fn [] (loop [i 0 acc 0] (if (< i 3) (recur (inc i) (+ acc (binding [*d* (+ i 10)] *d*))) acc))))",
            )
            .unwrap();
        for (source, expected) in [
            // Invariant reads, hoisted.
            ("(sum-x 5)", "10"),
            ("(sum-a)", "10"),
            // A call in the loop may redefine the var: read every iteration.
            ("(sum-y)", "9"),
            // A dynamic scope in the loop: read every iteration.
            ("(sum-d)", "33"),
            // A loop that runs no iterations reads nothing it needs.
            ("(sum-x 0)", "0"),
        ] {
            assert_eq!(printed(&mut session, source), expected, "{source}");
        }
        // Redefining the var between calls is seen by the next call.
        session.eval("(def x 7)").unwrap();
        assert_eq!(printed(&mut session, "(sum-x 2)"), "14");
    }
}
