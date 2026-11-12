//! `(aget a i)` and `(aset a i v)` on packed primitive arrays read and write
//! the storage directly for an i31 index inside the length (#389), and aset
//! for the values whose conversion is exact; everything else takes the full
//! conversion. Expected values are `ebda997f`'s, before the direct path.
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
fn packed_aget_and_aset_keep_their_results() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        session.set_operation_fuel(100_000_000);
        for (source, expected) in [
            (r#"(let [a (long-array [1 2 3])] [(aget a 0) (aget a 2) (aget a 1.7) (aset a 1 9) (aset a 2 1.5) (vec a)])"#, r#"[1 3 2 9 1 [1 9 1]]"#),
            (r#"(let [a (int-array [1 2 3])] [(aget a 0) (aset a 0 7) (aset a 1 -5) (vec a)])"#, r#"[1 7 -5 [7 -5 3]]"#),
            (r#"(let [a (double-array [1.5 2.5])] [(aget a 0) (aset a 0 5) (aset a 1 0.25) (vec a) (aset a 0 3000000000) (vec a)])"#, r#"[1.5 5.0 0.25 [5.0 0.25] 3.0E9 [3.0E9 0.25]]"#),
            (r#"(let [a (byte-array [1 2])] [(aget a 0) (aset a 0 100) (vec a)])"#, r#"[1 100 [100 2]]"#),
            (r#"(let [a (short-array [1 2])] [(aget a 1) (aset a 1 300) (vec a)])"#, r#"[2 300 [1 300]]"#),
            (r#"(let [a (float-array [1.5 2.5])] [(aget a 0) (aset a 0 0.1) (vec a)])"#, r#"[1.5 0.10000000149011612 [0.10000000149011612 2.5]]"#),
            (r#"(let [a (long-array [1 2])] [(aset a 0 3000000000) (aget a 0) (aset a 1 -1073741824) (aget a 1)])"#, r#"[3000000000 3000000000 -1073741824 -1073741824]"#),
            (r#"(let [a (long-array 2)] (try (aget a 2) (catch :default e (ex-message e))))"#, r#""Index 2 out of bounds for length 2""#),
            (r#"(let [a (long-array 2)] (try (aget a -1) (catch :default e (ex-message e))))"#, r#""Index -1 out of bounds for length 2""#),
            (r#"(let [a (double-array 2)] (try (aset a 5 1.0) (catch :default e (ex-message e))))"#, r#""Index 5 out of bounds for length 2""#),
            (r#"(let [a (int-array 2)] (try (aset a 0 3000000000) (catch :default e (ex-message e))))"#, r#""integer overflow""#),
            (r#"(let [a (byte-array 2)] (try (aset a 0 200) (catch :default e (ex-message e))))"#, r#""Value out of range for byte: 200""#),
            (r#"(let [a (long-array 2)] (try (aset a 0 "x") (catch :default e (ex-message e))))"#, r#""long requires a number operand""#),
            (r#"(let [a (double-array 2)] (try (aset a 0 nil) (catch :default e (ex-message e))))"#, r#""double requires a number operand""#),
            (r#"(let [a (long-array 3)] (try (aget a 3000000000) (catch :default e (ex-message e))))"#, r#""integer overflow""#),
            (r#"(let [a (to-array [1 2 3])] [(aget a 1) (aset a 1 :x) (vec a)])"#, r#"[2 :x [1 :x 3]]"#),
            (r#"(let [a (long-array (range 100))] (loop [i 0 s 0] (if (< i 100) (recur (inc i) (+ s (aget a i))) s)))"#, r#"4950"#),
            (r#"(let [a (double-array 100)] (loop [i 0] (when (< i 100) (aset a i i) (recur (inc i)))) (reduce + a))"#, r#"4950.0"#),
        ] {
            assert_eq!(printed(&mut session, source), expected, "{source}");
        }
    }
}
