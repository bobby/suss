//! The default printer (#304): type values print their names, and values that
//! are not data print as Clojure's `#object[...]` and `#error {...}` forms, in
//! both compiled phases and in components. Expected text is Clojure's
//! (`clojure/core_print.clj`), with Suss's type names.
#[path = "support/compilation_fuel.rs"]
mod compilation_fuel;

use std::process::Command;
use suss_cli::{
    portable_macro_data::FormBridge,
    portable_macros::CompiledMacros,
    portable_session::{Session, SessionOptions},
};
use suss_reader::edn::{self, Options, Value};
use suss_reader::forms::Kind;

/// The string a source expression evaluates to.
fn text(session: &mut Session, macros: &mut CompiledMacros, source: &str) -> String {
    let value = session
        .eval_with_macros(source, macros)
        .unwrap_or_else(|error| panic!("{source}: {error:?}"));
    session.collect().unwrap();
    let bridge = FormBridge::new(session).unwrap();
    match &bridge.read(session, &value, 0..1).unwrap().kind {
        Kind::String(units) => String::from_utf16(units).unwrap(),
        kind => panic!("{source}: expected a string: {kind:?}"),
    }
}

/// A Runtime and a Macro session, each with a macro host, in a namespace
/// whose name Clojure munges, with a few types.
fn sessions() -> Vec<(Session, CompiledMacros)> {
    let options = SessionOptions { source_paths: Vec::new(), fuel_per_operation: 1_000_000_000 };
    [Session::new_repl_with_options(options.clone()), Session::new_macro_with_options(options)]
        .into_iter()
        .map(|session| {
            let mut session = session.unwrap();
            let mut macros = CompiledMacros::new().unwrap();
            macros.set_operation_fuel(compilation_fuel::COMPILATION_FUEL);
            for source in [
                "(ns my-ns.printing)",
                "(deftype T [a])",
                "(deftype U [] Object (toString [_] \"u\"))",
                "(deftype M [m] IMeta (-meta [_] m))",
                "(deftype a-b? [])",
                "(defrecord R [a b])",
            ] {
                session.eval_with_macros(source, &mut macros).unwrap_or_else(|error| panic!("{source}: {error:?}"));
            }
            (session, macros)
        })
        .collect()
}

/// Replaces each hexadecimal hash after `0x` or `@` with `H`, so expected
/// text does not depend on identity hashes.
fn hashes_hidden(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(at) = rest.find(|c| c == '@' || c == 'x') {
        let (head, tail) = rest.split_at(at + 1);
        out.push_str(head);
        let marker = head.ends_with('@') || head.ends_with("0x");
        let digits = tail.find(|c: char| !c.is_ascii_hexdigit() || c.is_ascii_uppercase()).unwrap_or(tail.len());
        if marker && digits > 0 {
            out.push('H');
            rest = &tail[digits..];
        } else {
            rest = tail;
        }
    }
    out.push_str(rest);
    out
}

/// Each expression evaluates to the expected string, with hashes hidden, in
/// both phases.
fn check(cases: &[(&str, &str)]) {
    for (mut session, mut macros) in sessions() {
        for (source, expected) in cases {
            assert_eq!(hashes_hidden(&text(&mut session, &mut macros, source)), *expected, "{source}");
        }
    }
}

#[test]
fn type_values_print_their_class_names() {
    check(&[
        ("(pr-str (type []))", "suss.core.PersistentVector"),
        ("(pr-str (type {}))", "suss.core.PersistentArrayMap"),
        ("(pr-str T)", "my_ns.printing.T"),
        ("(pr-str (type (T. 1)))", "my_ns.printing.T"),
        ("(pr-str R)", "my_ns.printing.R"),
        ("(pr-str a-b?)", "my_ns.printing.a_b_QMARK_"),
        ("(pr-str [T])", "[my_ns.printing.T]"),
        ("(print-str T)", "my_ns.printing.T"),
        ("(pr-str (type (ex-info \"x\" {})))", "suss.core.ExceptionInfo"),
        // Clojure's Class/toString.
        ("(str T)", "class my_ns.printing.T"),
        ("(str (type []))", "class suss.core.PersistentVector"),
    ]);
}

#[test]
fn records_print_and_read_back_under_their_class_names() {
    check(&[
        ("(pr-str (->R 1 2))", "#my_ns.printing.R{:a 1, :b 2}"),
        ("(str (= (->R 1 [2]) (read-string (pr-str (->R 1 [2])))))", "true"),
        ("(binding [*print-dup* true] (pr-str (read-string (binding [*print-dup* true] (pr-str (->R 1 2))))))", "#my_ns.printing.R[1, 2]"),
        ("(str (->R 1 2))", "my_ns.printing.R@H"),
        // The unmunged name is not a class name, as in Clojure.
        ("(try (read-string \"#my-ns.printing.R{:a 1}\") (catch :default e (ex-message e)))", "No record type for tag my-ns.printing.R"),
    ]);
}

#[test]
fn objects_print_as_tagged_objects() {
    check(&[
        ("(pr-str (T. 1))", "#object[my_ns.printing.T 0xH \"my_ns.printing.T@H\"]"),
        ("(str (T. 1))", "my_ns.printing.T@H"),
        ("(print-str (T. 1))", "#object[my_ns.printing.T 0xH my_ns.printing.T@H]"),
        ("(pr-str [(T. 1)])", "[#object[my_ns.printing.T 0xH \"my_ns.printing.T@H\"]]"),
        // A type's own toString is its representation.
        ("(pr-str (U.))", "#object[my_ns.printing.U 0xH \"u\"]"),
        ("(str (U.))", "u"),
        ("(pr-str (a-b?.))", "#object[my_ns.printing.a_b_QMARK_ 0xH \"my_ns.printing.a_b_QMARK_@H\"]"),
        ("(pr-str (transient []))", "#object[suss.core.TransientVector 0xH \"suss.core.TransientVector@H\"]"),
        ("(pr-str (transient {}))", "#object[suss.core.TransientArrayMap 0xH \"suss.core.TransientArrayMap@H\"]"),
        // Metadata prints first, as print-tagged-object prints it.
        ("(binding [*print-meta* true] (pr-str (M. {:k 1})))", "^{:k 1} #object[my_ns.printing.M 0xH \"my_ns.printing.M@H\"]"),
        ("(pr-str (M. {:k 1}))", "#object[my_ns.printing.M 0xH \"my_ns.printing.M@H\"]"),
    ]);
    // The identity hash and the hash agree for a type without IHash, as
    // identityHashCode and hashCode do for a Clojure deftype.
    for (mut session, mut macros) in sessions() {
        let printed = text(&mut session, &mut macros, "(let [t (T. 1)] (pr-str [t (hash t)]))");
        let hex = printed.split("0x").nth(1).unwrap().split(' ').next().unwrap().to_owned();
        let hash: i64 = printed.rsplit(' ').next().unwrap().trim_end_matches(']').parse().unwrap();
        assert_eq!(u32::from_str_radix(&hex, 16).unwrap() as i64, hash & 0xffff_ffff, "{printed}");
        assert!(printed.contains(&format!("@{hex}\"")), "{printed}");
    }
}

#[test]
fn references_print_their_state() {
    check(&[
        ("(pr-str (reduced 1))", "#object[suss.core.Reduced 0xH {:status :ready, :val 1}]"),
        ("(pr-str (reduced [1 (T. 2)]))", "#object[suss.core.Reduced 0xH {:status :ready, :val [1 #object[my_ns.printing.T 0xH \"my_ns.printing.T@H\"]]}]"),
        ("(pr-str (atom 1))", "#object[suss.core.Atom 0xH {:status :ready, :val 1}]"),
        ("(pr-str (atom {:a [1]}))", "#object[suss.core.Atom 0xH {:status :ready, :val {:a [1]}}]"),
        // The state is a map, so *print-length* limits its entries too.
        ("(binding [*print-length* 1] (pr-str (atom [1 2])))", "#object[suss.core.Atom 0xH {:status :ready, ...}]"),
        ("(binding [*print-length* 2] (pr-str (atom [1 2 3])))", "#object[suss.core.Atom 0xH {:status :ready, :val [1 2 ...]}]"),
        ("(binding [*print-level* 1] (pr-str (atom [1])))", "#object[suss.core.Atom 0xH {:status :ready, :val #}]"),
    ]);
}

#[test]
fn exceptions_print_as_clojure_errors() {
    check(&[
        (
            "(pr-str (ex-info \"x\" {}))",
            "#error {\n :cause \"x\"\n :data {}\n :via\n [{:type suss.core.ExceptionInfo\n   :message \"x\"\n   :data {}}]\n :trace\n []}",
        ),
        (
            "(pr-str (ex-info \"outer\" {:a 1} (ex-info \"inner\" {:b [2]})))",
            "#error {\n :cause \"inner\"\n :data {:b [2]}\n :via\n [{:type suss.core.ExceptionInfo\n   :message \"outer\"\n   :data {:a 1}}\n  {:type suss.core.ExceptionInfo\n   :message \"inner\"\n   :data {:b [2]}}]\n :trace\n []}",
        ),
        (
            "(pr-str [(ex-info \"x\" {:k \"v\"})])",
            "[#error {\n :cause \"x\"\n :data {:k \"v\"}\n :via\n [{:type suss.core.ExceptionInfo\n   :message \"x\"\n   :data {:k \"v\"}}]\n :trace\n []}]",
        ),
    ]);
}

#[test]
fn printed_objects_do_not_read_back() {
    check(&[
        (
            "(try (read-string (pr-str (T. 1))) (catch :default e (ex-message e)))",
            "No reader function for tag object",
        ),
        (
            "(try (read-string (pr-str (ex-info \"x\" {}))) (catch :default e (ex-message e)))",
            "No reader function for tag error",
        ),
        (
            "(try (read-string (pr-str (atom 1))) (catch :default e (ex-message e)))",
            "No reader function for tag object",
        ),
    ]);
}

#[test]
fn components_print_objects_and_no_hidden_imports() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join("src/app")).unwrap();
    std::fs::write(
        root.path().join("src/app/my_objects.sus"),
        concat!(
            "(ns app.my-objects)\n",
            "(deftype Box [x])\n",
            "(defn types [] (pr-str [Box (type [])]))\n",
            "(defn objects [] (pr-str [(Box. 1) (reduced 2)]))\n",
            "(defn errors [] (pr-str (ex-info \"x\" {:k 1})))\n",
        ),
    )
    .unwrap();
    std::fs::write(
        root.path().join("objects.wit"),
        "package test:objects;\nworld objects {\n export types: func() -> string;\n export objects: func() -> string;\n export errors: func() -> string;\n}\n",
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_suss"))
        .current_dir(root.path())
        .args(["compile", "-n", "app.my-objects", "--src", "src", "-w", "objects.wit", "--wit-world", "objects", "-o", "objects.wasm"])
        .arg("--compile-fuel")
        .arg(compilation_fuel::COMPILATION_FUEL.to_string())
        .args(["--export", "types=app.my-objects/types", "--export", "objects=app.my-objects/objects", "--export", "errors=app.my-objects/errors"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    // The world imports nothing, so neither does the component (spec §8).
    let wasm = std::fs::read(root.path().join("objects.wasm")).unwrap();
    let mut imports = 0;
    for payload in wasmparser::Parser::new(0).parse_all(&wasm) {
        if let wasmparser::Payload::ComponentImportSection(section) = payload.unwrap() {
            imports += section.count();
        }
    }
    assert_eq!(imports, 0, "printing must not add a hidden import");
    for (export, expected) in [
        ("types", "[app.my_objects.Box suss.core.PersistentVector]"),
        ("objects", "[#object[app.my_objects.Box 0xH \"app.my_objects.Box@H\"] #object[suss.core.Reduced 0xH {:status :ready, :val 2}]]"),
        ("errors", "#error {\n :cause \"x\"\n :data {:k 1}\n :via\n [{:type suss.core.ExceptionInfo\n   :message \"x\"\n   :data {:k 1}}]\n :trace\n []}"),
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_suss"))
            .current_dir(root.path())
            .args(["run", "objects.wasm", "--invoke", export])
            .output()
            .unwrap();
        assert!(output.status.success(), "{export}: {}", String::from_utf8_lossy(&output.stderr));
        let out = String::from_utf8(output.stdout).unwrap();
        let printed = match edn::read_one(out.trim(), &Options::default()) {
            Ok(Value::String(text)) => text,
            _ => out.trim().to_owned(),
        };
        assert_eq!(hashes_hidden(&printed), expected, "{export}");
    }
}
