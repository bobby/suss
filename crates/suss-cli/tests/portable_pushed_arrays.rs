//! Object arrays built by push keep a dense prefix (#348): appending at the
//! prefix's end grows it, sparse entries it then covers move into it, and
//! holes, sparse writes, truncation and pop behave as before.
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
fn arrays_built_by_push_read_every_element_and_keep_holes() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.set_operation_fuel(100_000_000);
        for (source, expected) in [
            // to-array pushes each element.
            ("(let [a (to-array (range 1024))] [(alength a) (aget a 0) (aget a 1023) (reduce + a)])", "[1024 0 1023 523776]"),
            ("(let [a (array)] (.push a 1) (.push a 2) (aset a 5 9) [(alength a) (vec a)])", "[6 [1 2 nil nil nil 9]]"),
            // A sparse entry the grown prefix covers moves into it.
            ("(let [a (array)] (aset a 3 :x) (.push a 7) [(alength a) (vec a)])", "[5 [nil nil nil :x 7]]"),
            ("(let [a (to-array [1 2])] (aset a 10 :z) (.push a 3) [(alength a) (aget a 10) (aget a 11)])", "[12 :z 3]"),
            // Truncation, then growth again.
            ("(let [a (to-array (range 10))] (set! (.-length a) 3) (.push a :p) (vec a))", "[0 1 2 :p]"),
            ("(let [a (array)] (.push a 1) (.push a 2) (.pop a) (.push a 3) (vec a))", "[1 3]"),
            // Capacity past the length never shows a popped value.
            ("(let [a (array)] (.push a 1) (.push a 2) (.pop a) (set! (.-length a) 3) (vec a))", "[1 nil nil]"),
        ] {
            assert_eq!(printed(&mut session, source), expected, "{source}");
        }
    }
}
