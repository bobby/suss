//! The REPL displays each result as the guest's `pr-str` prints it, a `def` as
//! its var, and a thrown error by its message (#288). Expected text was
//! compared with Clojure 1.12.6's REPL where Clojure has the value.
use std::io::Write;
use std::process::{Command, Stdio};

fn repl(source: &str) -> (String, String) {
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
        .write_all(source.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "{output:?}");
    (
        String::from_utf8(output.stdout).unwrap(),
        String::from_utf8(output.stderr).unwrap(),
    )
}

#[test]
fn collections_seqs_records_types_and_arrays_display_as_pr_str_prints_them() {
    let (out, err) = repl(concat!(
        "(range 3)\n",
        "(sorted-map :a 1)\n",
        "(sorted-set 1 2)\n",
        "(first (sorted-map :a 1))\n",
        "(subvec [1 2 3] 1)\n",
        "(defrecord P [x])\n",
        "(->P 1)\n",
        "(deftype T [])\n",
        "T\n",
        "(conj (.-EMPTY PersistentQueue) 1)\n",
        "(array 1 2)\n",
        "(defprotocol Pr (m [x]))\n",
        "(atom 1)\n",
        "\"a\\nb\"\n",
    ));
    assert_eq!(err, "");
    assert_eq!(
        out,
        concat!(
            "(0 1 2)\n",
            "{:a 1}\n",
            "#{1 2}\n",
            "[:a 1]\n",
            "[2 3]\n",
            "user.P\n",
            "#user.P{:x 1}\n",
            "user.T\n",
            "user.T\n",
            "#queue [1]\n",
            "#js [1 2]\n",
            "Pr\n",
            "#object[suss.core.Atom 0x1 {:status :ready, :val 1}]\n",
            "\"a\\nb\"\n",
        )
    );
}

#[test]
fn definitions_display_their_var_without_printing_the_value() {
    let (out, err) = repl(concat!(
        "(def s (sorted-set 1))\n",
        "(defn f [] 1)\n",
        "(declare d)\n",
        "(defmacro mac [] 1)\n",
        "(defmulti mm identity)\n",
        "(defmulti mm identity)\n",
        "(let [] (def z 1))\n",
        "(do (def y 1) 2)\n",
        // Displaying the var doesn't realize the sequence.
        "(def lz (map (fn [x] (throw (ex-info \"realized\" {}))) [1]))\n",
        "s\n",
    ));
    assert_eq!(err, "");
    assert_eq!(
        out,
        concat!(
            "#'user/s\n",
            "#'user/f\n",
            "#'user/d\n",
            "#'user/mac\n",
            "#'user/mm\n",
            "nil\n",
            "#'user/z\n",
            "2\n",
            "#'user/lz\n",
            "#{1}\n",
        )
    );
}

#[test]
fn unbound_names_and_errors_are_language_errors() {
    let (out, err) = repl(concat!(
        "(def bad (throw (ex-info \"boom\" {})))\n",
        "bad\n",
        "(bad)\n",
        "(declare d)\n",
        "[d]\n",
        "(throw \"s\")\n",
        "(throw 31)\n",
        "(+ 1 2)\n",
    ));
    assert_eq!(out, "#<undefined>\n#'user/d\n[nil]\n3\n");
    assert_eq!(
        err,
        concat!(
            "Error: Uncaught language exception: boom\n",
            "Error: Uncaught language exception: Not callable\n",
            "Error: Uncaught language exception: \"s\"\n",
            "Error: Uncaught language exception: 31\n",
        )
    );
}

#[test]
fn futures_display_without_awaiting_inside_collections() {
    let (out, err) = repl(concat!(
        "(def p (suss.internal.async/pending))\n",
        "p\n",
        "[p]\n",
        "(pr-str {:p p})\n",
    ));
    assert_eq!(err, "");
    assert_eq!(
        out,
        "#'user/p\n#<future pending>\n[#<future pending>]\n\"{:p #<future pending>}\"\n"
    );
}
