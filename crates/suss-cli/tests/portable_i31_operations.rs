//! Addition, subtraction, bitwise operations and comparisons of two i31
//! longs run inline (#361); every other operand takes the runtime's
//! operation. Expected values are JVM Clojure 1.12.6's.
use suss_cli::portable_session::Session;

fn printed(session: &mut Session, source: &str) -> String {
    let value = session
        .eval(&format!("(pr-str {source})"))
        .unwrap_or_else(|error| panic!("{source}: {error}"));
    let units = session
        .inspect(&value, |mut store, value| {
            let array = value.unwrap_anyref().unwrap().as_array(&store)?.unwrap();
            Ok(array
                .elems(&mut store)?
                .map(|unit| unit.unwrap_i32() as u16)
                .collect::<Vec<_>>())
        })
        .unwrap();
    String::from_utf16(&units).unwrap()
}

#[test]
fn i31_operations_match_jvm_clojure() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.set_operation_fuel(100_000_000);
        for (source, expected) in [
            ("(bit-and -5 3)", "3"),
            ("(bit-or -1073741824 1)", "-1073741823"),
            ("(bit-xor 1073741823 -1)", "-1073741824"),
            // Shifts by 0..31 inline, and beyond 31 through the runtime.
            ("(bit-shift-right -7 1)", "-4"),
            ("(bit-shift-right -7 31)", "-1"),
            ("(bit-shift-right -7 32)", "-1"),
            ("(bit-shift-right 1000 63)", "0"),
            ("(bit-shift-right 1000 0)", "1000"),
            ("(bit-and 1099511627776 -1)", "1099511627776"),
            // A shift amount or value that isn't an i31 takes the runtime.
            ("(bit-shift-right 1099511627776 3)", "137438953472"),
            ("(bit-shift-right 1024 (long 3))", "128"),
            ("(bit-shift-right 1024 1099511627779)", "128"),
            ("(unsigned-bit-shift-right -8 1)", "9223372036854775804"),
            ("(unsigned-bit-shift-right 1024 3)", "128"),
            (
                "[(< 3 4) (< 4 3) (<= 3 3) (> -1 -2)]",
                "[true false true true]",
            ),
            // Sums past the i31 range take the runtime, which boxes them.
            (
                "[(+ 1073741823 1) (- -1073741824 1) (+ -3 5) (- 3 5)]",
                "[1073741824 -1073741825 2 -2]",
            ),
            (
                "[(+ 1 1.5) (- 1073741824 1) (+ 1073741823 1073741823)]",
                "[2.5 1073741823 2147483646]",
            ),
            // A double or a long beyond i31 takes the runtime's comparison.
            (
                "[(>= 1 1.5) (< 1073741823 1073741824) (> 5 4.5)]",
                "[false true true]",
            ),
        ] {
            assert_eq!(printed(&mut session, source), expected, "{source}");
        }
    }
}
