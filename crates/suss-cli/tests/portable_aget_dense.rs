//! `(aget a i)` reads a source array's dense prefix directly (#361); every
//! other case keeps `source-array-get`'s results. The expected values are the
//! full lookup's, observed before the direct read existed.
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
fn aget_keeps_source_array_lookup_results() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.set_operation_fuel(100_000_000);
        session
            .eval("(def a (make-array 4)) (aset a 1 :x) (def h (array 1 2 3)) (aset h 7 :far)")
            .unwrap();
        for (source, expected) in [
            // Dense, present: the direct read.
            ("(aget a 1)", ":x"),
            ("(aget h 2)", "3"),
            // A hole, past the length, negative, a double index, a sparse
            // element beyond the dense prefix, and a hole before it.
            ("(aget a 0)", "nil"),
            ("(aget a 9)", "nil"),
            ("(aget a -1)", "nil"),
            ("(aget (array 5 6) 1.0)", "6"),
            ("(aget h 7)", ":far"),
            ("(aget h 5)", "nil"),
            // Packed arrays, strings, nested indices and string keys.
            ("(aget (long-array [4 5]) 1)", "5"),
            ("(aget \"ab\" 1)", "\"b\""),
            ("(aget (array 1 (array 2 3)) 1 0)", "2"),
            ("(aget (array 1 2) \"1\")", "2"),
            ("(nth (vec (range 100)) 70)", "70"),
        ] {
            assert_eq!(printed(&mut session, source), expected, "{source}");
        }
    }
}
