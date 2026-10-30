//! Every shipped collection, seq and record type prints with `pr-str`,
//! `prn-str`, `print-str` and `str` as Clojure does (#285), in both compiled
//! phases. Expected text is Clojure's; host arrays seq as ClojureScript's do.
#[path = "support/compilation_fuel.rs"]
mod compilation_fuel;

use suss_cli::{portable_macros::CompiledMacros, portable_session::Session};

fn truthy(session: &mut Session, source: &str) {
    let value = session
        .eval(source)
        .unwrap_or_else(|error| panic!("{source}: {error:?}"));
    session.collect().unwrap();
    session
        .inspect(&value, |store, value| {
            let bits = value
                .unwrap_anyref()
                .and_then(|value| value.as_i31(&store).ok().flatten())
                .map(|value| value.get_u32());
            assert_eq!(bits, Some(4), "{source} is not true");
            Ok(())
        })
        .unwrap();
}

fn sessions() -> [Session; 2] {
    let mut sessions = [Session::new_repl().unwrap(), Session::new_macro().unwrap()];
    let mut macros = CompiledMacros::new().unwrap();
    macros.set_operation_fuel(compilation_fuel::COMPILATION_FUEL);
    for session in &mut sessions {
        session.set_operation_fuel(1_000_000_000);
        session
            .eval_with_macros(
                "(do (defrecord R [a]) (defrecord Q [a] Object (toString [_] \"custom\")) \
                     (defrecord S [a] Object (equals [_ o] false) Object (hashCode [_] 1)))",
                &mut macros,
            )
            .unwrap();
    }
    sessions
}

/// A Suss string literal for `text`.
fn literal(text: &str) -> String {
    format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n"))
}

const PRINTED: &[(&str, &str)] = &[
    // Range and IntegerRange.
    ("(range 3)", "(0 1 2)"),
    ("(seq (range 3))", "(0 1 2)"),
    ("(range 0)", "()"),
    ("(range 0 10 3)", "(0 3 6 9)"),
    ("(range 0.5 2)", "(0.5 1.5)"),
    // Sorted maps and sets, their entries, seqs and reverse seqs.
    ("(sorted-map :b 2 :a 1)", "{:a 1, :b 2}"),
    ("(sorted-map)", "{}"),
    ("(sorted-map-by > 1 2 3 4)", "{3 4, 1 2}"),
    ("(sorted-set 2 1)", "#{1 2}"),
    ("(sorted-set)", "#{}"),
    ("(sorted-set-by > 1 2)", "#{2 1}"),
    ("(first (sorted-map :a 1))", "[:a 1]"),
    ("(seq (sorted-map 1 2 3 4))", "([1 2] [3 4])"),
    ("(rseq (sorted-map 1 2 3 4))", "([3 4] [1 2])"),
    ("(seq (sorted-set 1 2))", "(1 2)"),
    ("(rseq (sorted-set 1 2))", "(2 1)"),
    ("(subseq (sorted-set 1 2 3) > 1)", "(2 3)"),
    ("(keys (sorted-map 1 2 3 4))", "(1 3)"),
    ("(vals (sorted-map 1 2 3 4))", "(2 4)"),
    ("(with-meta (sorted-set 1) {:m 1})", "#{1}"),
    ("(sorted-map :a (sorted-set 1) :b (range 2))", "{:a #{1}, :b (0 1)}"),
    // Records, including extended fields and nested values.
    ("(->R 1)", "#user.R{:a 1}"),
    ("(map->R {})", "#user.R{:a nil}"),
    ("(assoc (->R 1) :b 2)", "#user.R{:a 1, :b 2}"),
    ("(->R (sorted-set 1))", "#user.R{:a #{1}}"),
    ("[(->R 1)]", "[#user.R{:a 1}]"),
    // Other seq and collection types.
    ("(keys {1 2})", "(1)"),
    ("(vals {1 2})", "(2)"),
    ("(seq (array-map 1 2))", "([1 2])"),
    ("(seq (hash-map 1 2))", "([1 2])"),
    ("(seq [1 2])", "(1 2)"),
    ("(rseq [1 2])", "(2 1)"),
    ("(subvec [1 2 3] 1)", "[2 3]"),
    ("(cons 1 [2])", "(1 2)"),
    ("(map inc [0])", "(1)"),
    ("(seq (array 1 2))", "(1 2)"),
    ("(repeat 2 1)", "(1 1)"),
    ("(take 3 (iterate inc 1))", "(1 2 3)"),
];

#[test]
fn pr_str_and_prn_str_print_every_shipped_type() {
    for mut session in sessions() {
        for (form, expected) in PRINTED {
            truthy(&mut session, &format!("(= {} (pr-str {form}))", literal(expected)));
            truthy(&mut session, &format!("(= {} (prn-str {form}))", literal(&format!("{expected}\n"))));
        }
    }
}

#[test]
fn print_str_and_str_print_collections_without_quoting() {
    for mut session in sessions() {
        for (form, expected) in [
            ("(sorted-set \"a\")", "#{a}"),
            ("(range 3)", "(0 1 2)"),
            ("(sorted-map 1 \"b\")", "{1 b}"),
            ("(->R \"x\")", "#user.R{:a x}"),
        ] {
            truthy(&mut session, &format!("(= {} (print-str {form}))", literal(expected)));
        }
        for (form, expected) in [
            ("(sorted-set 2 1)", "#{1 2}"),
            ("(range 3)", "(0 1 2)"),
            ("(sorted-map 1 2)", "{1 2}"),
            ("(rseq (sorted-set 1 2))", "(2 1)"),
        ] {
            truthy(&mut session, &format!("(= {} (str {form}))", literal(expected)));
        }
    }
}

#[test]
fn str_of_a_record_is_its_label_and_hash_as_clojure_records_inherit() {
    for mut session in sessions() {
        // Clojure: Object.toString, the class name, "@" and the hash in hex.
        truthy(
            &mut session,
            "(let [r (->R 1) text (str r) hex \"0123456789abcdef\"] \
               (and (= \"user.R@\" (apply str (take 7 text))) \
                    (< 7 (count text) 16) \
                    (every? (fn [c] (some (fn [d] (= c d)) (map str hex))) (map str (drop 7 text))) \
                    (= text (str (->R 1))) \
                    (not= text (str (->R 2)))))",
        );
        // A record's own Object toString takes precedence.
        truthy(&mut session, "(= \"custom\" (str (->Q 1)))");
        // Repeated Object sections get the default toString once.
        truthy(&mut session, "(= \"user.S@\" (apply str (take 7 (str (->S 1)))))");
    }
}
