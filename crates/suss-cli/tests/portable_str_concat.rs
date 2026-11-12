//! `str` concatenates each value's str in order (#388): its variadic arity
//! through a loop with no StringBuffer, and a long's digits are built at
//! their exact length. Direct-linked calls are `compiled_str_calls`'. Expected values are
//! `ebda997f`'s, before the change.
use suss_cli::portable_session::Session;

fn printed(session: &mut Session, source: &str) -> String {
    let value = session
        .eval(&format!("(pr-str (try {source} (catch :default e [:error (ex-message e)])))"))
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
fn str_keeps_its_results() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.set_operation_fuel(100_000_000);
        for (source, expected) in [
            (r#"[(str) (str nil) (str nil nil) (str "a" nil "b") (str 1 2 3 4 5) (str :k 'sym \c) (str true false)]"#,
             r#"["" "" "" "ab" "12345" ":ksymc" "truefalse"]"#),
            (r#"[(str 1.5 -0.0 ##NaN ##Inf ##-Inf) (str 3000000000 -1073741825)]"#,
             r#"["1.5-0.0NaNInfinity-Infinity" "3000000000-1073741825"]"#),
            (r#"[(str [1 2] {:a 1} #{3} '(4 5)) (str (range 3)) (str "x" (list) "y")]"#,
             r#"["[1 2]{:a 1}#{3}(4 5)" "(0 1 2)" "x()y"]"#),
            ("(apply str (map str (range 20)))", r#""012345678910111213141516171819""#),
            (r#"(apply str "a" "b" "c" "d" "e" [])"#, r#""abcde""#),
            ("(let [a (atom 0) f (fn [] (swap! a inc) @a)] [(str (f) (f) (f) (f) (f)) @a])", r#"["12345" 5]"#),
            (r#"(str "héllo" "→" "𝄞")"#, r#""héllo→𝄞""#),
            ("[(str 0) (str 7) (str -7) (str 1234567890) (str -1073741824) (str 9223372036854775807) (str -9223372036854775808) (str 10) (str -10)]",
             r#"["0" "7" "-7" "1234567890" "-1073741824" "9223372036854775807" "-9223372036854775808" "10" "-10"]"#),
        ] {
            assert_eq!(printed(&mut session, source), expected, "{source}");
        }
    }
}
