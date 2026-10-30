//! The `#queue [...]` literal (#287) reads as a persistent queue, so printed
//! queues read back. Clojure has no queue literal; Suss adds ClojureScript's,
//! with Clojure's tagged-literal model: the queue is a constant, its elements
//! unevaluated. Checked in both compiled phases, through macros, in the REPL
//! and in source files.
#[path = "support/compilation_fuel.rs"]
mod compilation_fuel;

use std::io::Write;
use std::process::{Command, Stdio};
use suss_cli::{
    portable_macro_data::FormBridge,
    portable_macros::CompiledMacros,
    portable_session::{Session, SessionError, SessionValue},
};
use suss_reader::forms::Kind;

fn truthy(session: &mut Session, source: &str) {
    let value = session
        .eval(source)
        .unwrap_or_else(|error| panic!("{source}: {error:?}"));
    assert_true(session, &value, source);
}

fn assert_true(session: &mut Session, value: &SessionValue, source: &str) {
    session.collect().unwrap();
    session
        .inspect(value, |store, value| {
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
        session.set_operation_fuel(1_000_000_000);
    }
    sessions
}

/// A Suss string literal for `text`.
fn literal(text: &str) -> String {
    format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
}

/// Values whose printed form must read back as an equal queue.
const PRINTED: &[(&str, &str)] = &[
    ("#queue []", "#queue []"),
    ("#queue [1 2 3]", "#queue [1 2 3]"),
    ("(conj #queue [1] 2)", "#queue [1 2]"),
    ("(conj (pop #queue [1 2]) 3)", "#queue [2 3]"),
    ("(into #queue [] (range 40))", "#queue [0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21 22 23 24 25 26 27 28 29 30 31 32 33 34 35 36 37 38 39]"),
    ("#queue [:a \"b\" {:c [1]} #{2} nil]", "#queue [:a \"b\" {:c [1]} #{2} nil]"),
    ("[#queue [#queue []] {:q #queue [1]}]", "[#queue [#queue []] {:q #queue [1]}]"),
    ("(with-meta #queue [1] {:m 1})", "#queue [1]"),
    ("^{:m 1} #queue [1]", "#queue [1]"),
];

#[test]
fn printed_queues_read_back_as_equal_queues() {
    for mut session in sessions() {
        for (form, expected) in PRINTED {
            truthy(&mut session, &format!("(= {} (pr-str {form}))", literal(expected)));
            // The printed text, read as source, is an equal queue.
            truthy(&mut session, &format!("(= {form} {expected})"));
        }
    }
}

#[test]
fn queue_literals_are_constants_and_behave_as_queues() {
    for mut session in sessions() {
        for source in [
            // As a tagged literal's value in Clojure, nothing inside is evaluated.
            "(= '[1 (inc 1) x] (vec #queue [1 (inc 1) x]))",
            "(= '#queue [(inc 1)] #queue [(inc 1)])",
            "(= #queue [1 2] [1 2] '(1 2))",
            "(= 1 (peek #queue [1 2]))",
            "(= #queue [2] (pop #queue [1 2]))",
            "(= #queue [] (empty #queue [1]))",
            "(zero? (count #queue []))",
            "(= 3 (count #queue [1 2 3]))",
            "(= (list 1 2) (seq #queue [1 2]))",
            "(not= #queue [1 2] #queue [2 1])",
            "(= '{:m (inc 1)} (meta ^{:m (inc 1)} #queue []))",
            "(= {:m 1} (meta ^{:m 1} #queue [1]))",
        ] {
            truthy(&mut session, source);
        }
    }
}

#[test]
fn quoted_and_syntax_quoted_queues_are_queue_data() {
    for mut session in sessions() {
        for source in [
            "(= \"#queue [a (b)]\" (pr-str '#queue [a (b)]))",
            "(= 'a (peek '#queue [a (b)]))",
            "(= '(b) (second '#queue [a (b)]))",
            "(= '#queue [] #queue [])",
            "(= {:m 1} (meta '^{:m 1} #queue [x]))",
            "(= \"[#queue [x]]\" (pr-str '[#queue [x]]))",
            "(let [x 5] (= \"#queue [user/x 5]\" (pr-str `#queue [x ~x])))",
            "(= #queue [1 2 3] `#queue [1 ~@[2 3]])",
            "(= 1 (peek `#queue [1 ~@[2 3]]))",
        ] {
            truthy(&mut session, source);
        }
    }
}

#[test]
fn macros_receive_and_return_queues() {
    let mut macros = CompiledMacros::new().unwrap();
    macros.set_operation_fuel(compilation_fuel::COMPILATION_FUEL);
    for definition in [
        "(defmacro qcount [q] (count q))",
        "(defmacro qpeek [q] (list 'quote (peek q)))",
        "(defmacro qmeta [q] (list 'quote (meta q)))",
        "(defmacro qquote [q] (list 'quote q))",
        "(defmacro qmake [] (conj #queue [] '(+ 1 2) :k))",
    ] {
        macros.define(definition).unwrap();
    }
    for mut session in sessions() {
        for source in [
            "(= 2 (qcount #queue [x y]))",
            "(= 'x (qpeek #queue [x y]))",
            "(= {:k true} (qmeta ^:k #queue []))",
            "(= \"#queue [a (b)]\" (pr-str (qquote #queue [a (b)])))",
            "(= 'a (peek (qquote #queue [a])))",
            // A returned queue is a constant, as in Clojure: `(+ 1 2)` is not run.
            "(= '#queue [(+ 1 2) :k] (qmake))",
        ] {
            let value = session
                .eval_with_macros(source, &mut macros)
                .unwrap_or_else(|error| panic!("{source}: {error:?}"));
            assert_true(&mut session, &value, source);
        }
        // A queue value reads back as the literal's syntax.
        let value = session.eval("(conj #queue [1] 'a)").unwrap();
        session.collect().unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        let form = bridge.read(&mut session, &value, 0..1).unwrap();
        let Kind::Tagged { tag, form } = &form.kind else {
            panic!("{form:?}")
        };
        assert_eq!(tag.name, "queue");
        let Kind::Vector(items) = &form.kind else { panic!("{form:?}") };
        assert_eq!(items[0].kind, Kind::Number(1.0));
        assert!(matches!(&items[1].kind, Kind::Symbol(symbol) if symbol.name == "a"));
    }
}

#[test]
fn malformed_tagged_literals_are_reader_diagnostics_with_spans() {
    for mut session in sessions() {
        for (source, message, span) in [
            ("#queue 1", "Queue literal expects a vector for its elements.", 0..8),
            ("[#queue (1)]", "Queue literal expects a vector for its elements.", 1..11),
            ("#queue {}", "Queue literal expects a vector for its elements.", 0..9),
            ("#unknown [1]", "No reader function for tag unknown", 0..12),
        ] {
            let Err(SessionError::Compile(diagnostic)) = session.eval(source) else {
                panic!("{source} did not fail to read");
            };
            assert_eq!(diagnostic.message, message, "{source}");
            assert_eq!(diagnostic.span, span, "{source}");
        }
        // Queues in destructuring keys hash and compare as their elements.
        truthy(&mut session, "(= 2 (let [{a [1] b 2 c 3 d 4 e 5 f 6 g 7 h 8 i #queue [9]} {[1] 2 [9] 10}] a))");
        truthy(&mut session, "(= 10 (let [{a [1] b 2 c 3 d 4 e 5 f 6 g 7 h 8 i #queue [9]} {[1] 2 [9] 10}] i))");
        let Err(SessionError::Compile(_)) = session.eval("(let [{:keys #{a #queue []}} {}] a)") else {
            panic!("a queue in :keys did not fail to compile");
        };
        // The session stays usable.
        truthy(&mut session, "(= 1 (peek #queue [1]))");
    }
}

#[test]
fn the_repl_reads_and_prints_queue_literals() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_suss"))
        .arg("repl")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"#queue [1 2]\n(peek #queue [1 2])\n'#queue [a]\n#queue 1\n(conj #queue [] 3)\n")
        .unwrap();
    let output = child.wait_with_output().unwrap();
    let out = String::from_utf8(output.stdout).unwrap();
    let errors = String::from_utf8(output.stderr).unwrap();
    let lines = out.lines().filter(|line| *line != "...").collect::<Vec<_>>();
    assert_eq!(lines, ["#queue [1 2]", "1", "#queue [a]", "#queue [3]"], "{out}\n{errors}");
    assert!(errors.contains("Queue literal expects a vector for its elements."), "{errors}");
}

#[test]
fn source_files_read_queue_literals() {
    let root = tempfile::tempdir().unwrap();
    let file = root.path().join("app.sus");
    let run = |source: &str| {
        std::fs::write(&file, source).unwrap();
        Command::new(env!("CARGO_BIN_EXE_suss")).arg(&file).output().unwrap()
    };
    let output = run(concat!(
        "(ns app)\n",
        "(def q (conj #queue [1 2] 3))\n",
        "(when-not (and (= \"#queue [1 2 3]\" (pr-str q)) (= q #queue [1 2 3]) (= 1 (peek q)))\n",
        "  (throw (ex-info \"queue literal\" {})))\n",
    ));
    assert!(output.status.success(), "{output:?}");
    // The check above can fail.
    let output = run("(ns app)\n(when-not (= #queue [1] #queue [2]) (throw (ex-info \"differ\" {})))\n");
    assert!(!output.status.success(), "{output:?}");
    let output = run("(ns app)\n#queue 1\n");
    assert!(!output.status.success(), "{output:?}");
    let errors = String::from_utf8(output.stderr).unwrap();
    assert!(
        errors.contains("Queue literal expects a vector for its elements. at bytes 9..17"),
        "{errors}"
    );
}
