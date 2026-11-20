//! Core copy-loop behavior across guarded bulk copying and its fallback (#479).
use suss_cli::portable_session::Session;

fn printed(session: &mut Session, source: &str) -> String {
    // The backward helper is private to core; assertions still observe source
    // values, identity and effects through the ordinary compiled pipeline.
    let value = session.eval_privileged(&format!("(pr-str {source})"))
        .unwrap_or_else(|error| panic!("{source}: {error}"));
    let units = session.inspect(&value, |mut store, value| {
        let array = value.unwrap_anyref().unwrap().as_array(&store)?.unwrap();
        Ok(array.elems(&mut store)?.map(|unit| unit.unwrap_i32() as u16).collect::<Vec<_>>())
    }).unwrap();
    String::from_utf16(&units).unwrap()
}

#[test]
fn copies_preserve_each_sequential_overlap_direction_after_gc() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        for (helper, i, j, expected) in [
            ("array-copy", 0, 1, "[true [0 0 0 0 0 5]]"),
            ("array-copy", 1, 0, "[true [1 2 3 4 4 5]]"),
            ("array-copy-downward", 0, 1, "[true [0 0 1 2 3 5]]"),
            ("array-copy-downward", 1, 0, "[true [4 4 4 4 4 5]]"),
        ] {
            session.eval("(def copy-a (object-array [0 1 2 3 4 5]))").unwrap();
            session.collect().unwrap();
            assert_eq!(printed(&mut session, &format!(
                "(let [answer (suss.core/{helper} copy-a {i} copy-a {j} 4)]
                   [(identical? answer copy-a) (vec copy-a)])")), expected);
        }
    }
}

#[test]
fn native_copy_admits_valid_ranges_and_declines_without_writing() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        for direction in ["forward", "backward"] {
            assert_eq!(printed(&mut session, &format!(r#"
                (let [from (object-array [1 2]) to (object-array 2)
                      result (suss.bootstrap/object-array-copy-{direction} from 0 to 0 2)]
                  [(suss.bootstrap/undefined? result) (identical? result to) (vec to)])"#)),
                "[false true [1 2]]");
            assert_eq!(printed(&mut session, &format!(r#"
                (let [from (object-array [1 2]) to (object-array [9 9])
                      result (suss.bootstrap/object-array-copy-{direction} from 1 to 0 2)]
                  [(suss.bootstrap/undefined? result) (vec to)])"#)), "[true [9 9]]");
            assert_eq!(printed(&mut session, &format!(
                "(suss.bootstrap/object-array-copy-{direction} nil -1 :owner -1 0)")), ":owner");
        }
        for (direction, i, j) in [("forward", 0, 1), ("backward", 1, 0)] {
            assert_eq!(printed(&mut session, &format!(r#"
                (let [a (object-array [0 1 2])
                      result (suss.bootstrap/object-array-copy-{direction} a {i} a {j} 2)]
                  [(suss.bootstrap/undefined? result) (vec a)])"#)), "[true [0 1 2]]");
        }
    }
}

#[test]
fn copy_fallback_preserves_partial_writes_and_zero_length_behavior() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        for (helper, i, j, expected) in [
            ("array-copy", 1, 0, "[true [1 2 9]]"),
            ("array-copy", 0, 1, "[true [9 0 1]]"),
            ("array-copy-downward", -1, 0, "[true [9 0 1]]"),
            ("array-copy-downward", 0, 1, "[true [9 9 9]]"),
        ] {
            session.eval("(def copy-from (object-array [0 1 2])) (def copy-to (object-array [9 9 9]))").unwrap();
            assert_eq!(printed(&mut session, &format!(
                "[(try (suss.core/{helper} copy-from {i} copy-to {j} 3)
                       false (catch :default e true)) (vec copy-to)]")), expected);
        }
        for helper in ["array-copy", "array-copy-downward"] {
            assert_eq!(printed(&mut session, &format!(
                "(suss.core/{helper} nil -1 :owner -1 0)")), ":owner");
        }
        session.eval("(def copy-from (object-array [0 1 2])) (def copy-to (object-array [9 9 9]))").unwrap();
        assert_eq!(printed(&mut session,
            "(do (array-copy copy-from 0.75 copy-to 0.25 2) (vec copy-to))"), "[0 1 9]");
    }
}

#[test]
fn copies_keep_primitive_fallbacks_argument_order_and_element_identity() {
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        for helper in ["array-copy", "array-copy-downward"] {
            session.eval("(def copy-from (int-array [1 2 3])) (def copy-to (long-array 3))").unwrap();
            assert_eq!(printed(&mut session, &format!(
                "(let [answer (suss.core/{helper} copy-from 0 copy-to 0 3)]
                   [(identical? answer copy-to) (vec copy-to)])")), "[true [1 2 3]]");
        }
        session.eval("(def copy-element (object-array [7])) (def copy-from (object-array [copy-element]))
                      (def copy-to (object-array 1)) (def copy-events (atom []))").unwrap();
        session.collect().unwrap();
        assert_eq!(printed(&mut session, r#"
            (let [answer (array-copy
              (do (swap! copy-events conj :from) copy-from)
              (do (swap! copy-events conj :i) 0)
              (do (swap! copy-events conj :to) copy-to)
              (do (swap! copy-events conj :j) 0)
              (do (swap! copy-events conj :len) 1))]
              [(identical? answer copy-to) (identical? (aget copy-to 0) copy-element)
               @copy-events])"#), "[true true [:from :i :to :j :len]]");
    }
}
