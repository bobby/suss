//! Clojure comparisons are functions (#293, #294): inline `< <= > >= ==`
//! evaluate every operand exactly once, left to right, before comparing, in
//! both compiled phases.
use suss_cli::portable_session::{Session, SessionError};

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
    for session in &mut sessions {
        session.set_operation_fuel(100_000_000);
    }
    sessions
}

const OPERATIONS: [(&str, fn(i32, i32) -> bool); 5] = [
    ("<", |a, b| a < b),
    ("<=", |a, b| a <= b),
    (">", |a, b| a > b),
    (">=", |a, b| a >= b),
    ("==", |a, b| a == b),
];

#[test]
fn every_operand_is_evaluated_once_in_order() {
    let operands: [&[i32]; 10] = [
        &[1],
        &[1, 2],
        &[2, 1],
        &[1, 2, 3],
        &[2, 1, 3],
        &[1, 3, 2],
        &[1, 1, 1],
        &[3, 2, 1, 0],
        &[1, 2, 2, 3],
        &[0, 1, 0, 1],
    ];
    for mut session in sessions() {
        for (operation, holds) in OPERATIONS {
            for values in operands {
                let expected = values.windows(2).all(|pair| holds(pair[0], pair[1]));
                let arguments = values
                    .iter()
                    .enumerate()
                    .map(|(index, value)| format!("(do (swap! log conj {index}) {value})"))
                    .collect::<Vec<_>>()
                    .join(" ");
                let order = (0..values.len()).map(|index| index.to_string()).collect::<Vec<_>>();
                truthy(
                    &mut session,
                    &format!(
                        "(let [log (atom [])] \
                         (and (= {expected} ({operation} {arguments})) (= [{}] @log)))",
                        order.join(" ")
                    ),
                );
            }
        }
    }
}

#[test]
fn operand_effects_and_throws_happen_regardless_of_the_result() {
    for mut session in sessions() {
        for (operation, _) in OPERATIONS {
            for source in [
                // #294: a unary comparison evaluates its operand, unchecked.
                format!("(let [n (atom 0)] (and ({operation} (do (swap! n inc) :a)) (= 1 @n)))"),
                format!(
                    "(= :thrown (try ({operation} (throw (ex-info \"x\" {{}}))) \
                     (catch :default e :thrown)))"
                ),
                // #293: operands after the first false pair are evaluated, though
                // not compared, so a non-number there is not checked.
                format!(
                    "(let [n (atom 0)] \
                     (and (false? ({operation} 1 ##NaN (do (swap! n inc) :a))) (= 1 @n)))"
                ),
                format!(
                    "(= :thrown (try ({operation} 1 ##NaN (throw (ex-info \"x\" {{}}))) \
                     (catch :default e :thrown)))"
                ),
                // A middle operand's effect happens once even when both of its
                // pairs are compared.
                format!(
                    "(let [n (atom 0)] \
                     ({operation} 1 (do (swap! n inc) 1) 1) (= 1 @n))"
                ),
            ] {
                truthy(&mut session, &source);
            }
        }
        // Typed operands keep their results.
        truthy(&mut session, "(let [^number x 1] (and (< 0 x 2) (false? (< 2 x 3))))");
        truthy(
            &mut session,
            "(= \"< requires number operands\" \
             (try (< 1 2 :a) :returned (catch :default e (ex-message e))))",
        );
    }
}

#[test]
fn unary_operands_are_analyzed_like_any_other_expression() {
    // The pinned macro erased a unary operand, so a non-tail recur or an
    // unresolved symbol there compiled to true; Clojure rejects both.
    for mut session in sessions() {
        for (operation, _) in OPERATIONS {
            for source in [
                format!("(loop [] ({operation} (recur)))"),
                format!("({operation} comparison-operand-unresolved)"),
                format!("(loop [] ({operation} 1 (recur) 2))"),
            ] {
                assert!(
                    matches!(session.eval(&source), Err(SessionError::Compile(_))),
                    "{source} compiled"
                );
            }
        }
    }
}
