//! `suss analyze` and the analyzer's EDN output (#299 step 2).
use std::path::{Path, PathBuf};
use suss_compile::portable_analyze::{analyze_source, comparable_asts, emitted_source, hir_document};
use suss_reader::edn::print;

/// Analyzing the forms that a file's nodes emit gives equal nodes.
fn assert_round_trip(source: &str, path: &Path, source_paths: &[PathBuf]) {
    let fragments = analyze_source(source, path, source_paths, None, None).unwrap();
    let document = hir_document(&fragments, path).unwrap();
    let emitted = emitted_source(&document).unwrap();
    let again = analyze_source(&emitted, path, source_paths, None, None)
        .unwrap_or_else(|error| panic!("{}: {error}\n{emitted}", path.display()));
    let second = hir_document(&again, path).unwrap();
    let (a, b) = (print(&comparable_asts(&document)).unwrap(), print(&comparable_asts(&second)).unwrap());
    if a != b {
        let common = a.bytes().zip(b.bytes()).take_while(|(x, y)| x == y).count();
        let start = common.saturating_sub(400);
        panic!(
            "{}: round trip differs at byte {common}\nfirst:  {}\nsecond: {}\nemitted:\n{emitted}",
            path.display(),
            &a[start..(common + 400).min(a.len())],
            &b[start..(common + 400).min(b.len())],
        );
    }
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn samples_round_trip() {
    // game_of_life.sus and primes.sus use for and Math/sqrt, which Suss
    // does not support yet.
    let paths = match std::env::var("SUSS_ANALYZE_FILE") {
        Ok(path) => vec![PathBuf::from(path)],
        Err(_) => ["factorial", "fibonacci", "quicksort", "tree_traversal"]
            .iter()
            .map(|name| root().join(format!("samples/{name}.sus")))
            .collect(),
    };
    for path in paths {
        let source = std::fs::read_to_string(&path).unwrap();
        assert_round_trip(&source, &path, &[]);
    }
}

/// The expressions of the compiler's conformance corpus
/// (`reference/cljs-tests`), as one file.
#[test]
fn conformance_corpus_round_trips() {
    let mut source = String::new();
    let mut count = 0;
    for file in ["core", "collections"] {
        let text = std::fs::read_to_string(root().join(format!("reference/cljs-tests/{file}.sus"))).unwrap();
        let cases = suss_reader::edn::read_one(&text, &suss_reader::edn::Options::default()).unwrap();
        let suss_reader::edn::Value::Vector(cases) = cases else { panic!("{file}: not a vector") };
        for case in cases {
            let suss_reader::edn::Value::Map(case) = case else { panic!("{file}: not a map") };
            let get = |key: &str| case.get(&suss_reader::edn::Value::keyword(key));
            if get("skip").is_some() {
                continue;
            }
            if let Some(suss_reader::edn::Value::String(expr)) = get("expr") {
                source.push_str(expr);
                source.push('\n');
                count += 1;
            }
        }
    }
    assert!(count >= 200, "{count} cases");
    assert_round_trip(&source, &root().join("reference/cljs-tests/corpus.sus"), &[]);
}

fn get<'v>(value: &'v suss_reader::edn::Value, key: &str) -> &'v suss_reader::edn::Value {
    match value {
        suss_reader::edn::Value::Map(map) => map.get(&suss_reader::edn::Value::keyword(key)).unwrap_or_else(|| panic!("no {key}")),
        _ => panic!("not a map: {key}"),
    }
}

/// Nodes reachable through `:children`, as a generic walker counts them.
fn count_nodes(node: &suss_reader::edn::Value) -> usize {
    let suss_reader::edn::Value::Vector(children) = get(node, "children") else { panic!("children") };
    1 + children
        .iter()
        .map(|key| {
            let suss_reader::edn::Value::Keyword(key) = key else { panic!("child key") };
            match get(node, &key.name) {
                suss_reader::edn::Value::Vector(items) => items.iter().map(count_nodes).sum(),
                child => count_nodes(child),
            }
        })
        .sum::<usize>()
}

fn asts(document: &suss_reader::edn::Value) -> &[suss_reader::edn::Value] {
    match get(document, "asts") {
        suss_reader::edn::Value::Vector(asts) => asts,
        _ => panic!("asts"),
    }
}

/// A pass edits the data and the emitted forms follow it: here every
/// constant 1 becomes 2.
#[test]
fn emitted_forms_follow_edited_nodes() {
    use suss_reader::edn::Value;
    fn edit(value: &Value) -> Value {
        match value {
            Value::Map(map) if map.get(&Value::keyword("op")) == Some(&Value::keyword("const"))
                && map.get(&Value::keyword("val")) == Some(&Value::Number(1.0)) =>
            {
                let mut map = map.clone();
                map.insert(Value::keyword("val"), Value::Number(2.0));
                Value::Map(map)
            }
            Value::Map(map) => Value::Map(map.iter().map(|(k, v)| (k.clone(), edit(v))).collect()),
            Value::Vector(items) => Value::Vector(items.iter().map(edit).collect()),
            other => other.clone(),
        }
    }
    let path = root().join("samples/factorial.sus");
    let source = std::fs::read_to_string(&path).unwrap();
    let document = hir_document(&analyze_source(&source, &path, &[], None, None).unwrap(), &path).unwrap();
    let emitted = emitted_source(&edit(&document)).unwrap();
    assert!(emitted.contains(" acc 2 ") && emitted.contains("(suss.core/reduce suss.core/* 2 "), "{emitted}");
    let second = hir_document(&analyze_source(&emitted, &path, &[], None, None).unwrap(), &path).unwrap();
    assert_ne!(print(&comparable_asts(&document)).unwrap(), print(&comparable_asts(&second)).unwrap());
    assert_eq!(
        print(&comparable_asts(&edit(&document))).unwrap(),
        print(&comparable_asts(&second)).unwrap(),
    );
}

/// `suss analyze` prints both documents, which read back as EDN, and a Suss
/// program reads the analysis with `suss.edn/read-string` and walks it.
#[test]
fn cli_prints_documents_that_suss_reads() {
    use suss_cli::{portable_macros::CompiledMacros, portable_session::{Session, SessionOptions}};
    use suss_reader::edn::{Options, Value, read_one};
    let path = root().join("samples/factorial.sus");
    let run = |emit: &str| {
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_suss"))
            .args(["--no-cache", "analyze", "--emit", emit])
            .arg(&path)
            .output()
            .unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        String::from_utf8(output.stdout).unwrap()
    };
    let forms = read_one(&run("forms"), &Options::default()).unwrap();
    assert_eq!(get(&forms, "suss.analyzer/version"), &Value::Number(1.0));
    assert_eq!(get(&forms, "suss.analyzer/kind"), &Value::keyword("forms"));
    let Value::Vector(trees) = get(&forms, "suss/source") else { panic!("trees") };
    assert_eq!(get(&trees[0], "line"), &Value::Number(8.0));
    assert_eq!(get(&trees[0], "column"), &Value::Number(1.0));
    let text = run("hir");
    let document = read_one(&text, &Options::default()).unwrap();
    assert_eq!(get(&document, "suss.analyzer/kind"), &Value::keyword("hir"));
    let nodes: usize = asts(&document).iter().map(count_nodes).sum();
    assert!(nodes > 40, "{nodes}");
    let Value::Map(namespaces) = get(get(&document, "global-env"), "namespaces") else { panic!("namespaces") };
    let user = &namespaces[&Value::Symbol(suss_reader::Symbol::new("user"))];
    let Value::Map(defs) = get(user, "defs") else { panic!("defs") };
    assert!(defs.contains_key(&Value::Symbol(suss_reader::Symbol::new("factorial"))));

    let mut session = Session::new_repl_with_options(SessionOptions {
        source_paths: Vec::new(),
        fuel_per_operation: 10_000_000_000,
    })
    .unwrap();
    let mut macros = CompiledMacros::new().unwrap();
    session.eval_with_macros("(ns user.tooling (:require [suss.edn :as edn]))", &mut macros).unwrap();
    let literal = suss_reader::edn::print(&Value::String(text.trim_end().to_owned())).unwrap();
    let program = format!(
        "(let [d (edn/read-string {literal})
               walk (fn walk [n]
                      (inc (reduce + 0 (map (fn [k] (let [c (get n k)] (if (vector? c) (reduce + 0 (map walk c)) (walk c))))
                                            (:children n)))))]
           (= [1 :hir {nodes}] [(:suss.analyzer/version d) (:suss.analyzer/kind d) (reduce + 0 (map walk (:asts d)))]))"
    );
    let value = session.eval_with_macros(&program, &mut macros).unwrap_or_else(|error| panic!("{error:?}"));
    session.collect().unwrap();
    session
        .inspect(&value, |store, value| {
            let bits = value.unwrap_anyref().and_then(|v| v.as_i31(&store).ok().flatten()).map(|v| v.get_u32());
            assert_eq!(bits, Some(4), "the Suss walk disagrees");
            Ok(())
        })
        .unwrap();
}

/// The node shapes of the special forms, with locals and loops numbered in
/// walk order; and the same document from a second analysis.
#[test]
fn special_forms_have_tools_analyzer_shapes() {
    use suss_reader::edn::{Options, read_one};
    let path = root().join("scratch/shapes.sus");
    let source = "(ns shapes.core)
(defn f [a & more]
  (let [b (inc a)]
    (loop [i b]
      (if (pos? i) (recur (dec i)) (try (throw (ex-info \"x\" {:i i})) (catch :default e [e more]) (finally nil))))))
";
    let document = hir_document(&analyze_source(source, &path, &[], None, None).unwrap(), &path).unwrap();
    let again = hir_document(&analyze_source(source, &path, &[], None, None).unwrap(), &path).unwrap();
    assert_eq!(print(&document).unwrap(), print(&again).unwrap(), "analysis output is deterministic");
    let expected = read_one(
        "[{:op :suss/ns, :name shapes.core}
          {:op :def, :var shapes.core/f, :children [:init],
           :init {:op :fn, :children [:methods],
                  :methods [{:op :fn-method, :loop-id 0, :variadic? true, :fixed-arity 1, :children [:params :body],
                             :params [{:op :binding, :name a, :id 0, :local :arg, :arg-id 0}
                                      {:op :binding, :name more, :id 1, :local :arg, :arg-id 1}]}]}}]",
        &Options::default(),
    )
    .unwrap();
    // Each expected entry is a subset of the node.
    fn subset(expected: &suss_reader::edn::Value, actual: &suss_reader::edn::Value) -> bool {
        use suss_reader::edn::Value;
        match (expected, actual) {
            (Value::Map(e), Value::Map(a)) => e.iter().all(|(k, v)| a.get(k).is_some_and(|a| subset(v, a))),
            (Value::Vector(e), Value::Vector(a)) => e.len() == a.len() && e.iter().zip(a).all(|(e, a)| subset(e, a)),
            (e, a) => e == a,
        }
    }
    let actual = suss_reader::edn::Value::Vector(asts(&document).to_vec());
    assert!(subset(&expected, &actual), "{}", print(&actual).unwrap());
    let text = print(&document).unwrap();
    for part in [
        ":op :let", ":op :loop", ":loop-id 1", ":op :recur", ":op :try", ":op :catch",
        ":local :catch", ":class {:children [], :form :default, :op :const, :type :class, :val :default}",
        ":op :throw", ":finally {", ":op :map", ":op :vector", ":body? true",
        ":op :let, :tag ignore, :suss/source {:column 3, :end-column 115, :end-line 5, :line 3}", ":env {:context :ctx/return, :locals {a 0, b 2, more 1}, :ns shapes.core, :phase :runtime}",
    ] {
        assert!(text.contains(part), "missing {part}\n{text}");
    }
}

/// `suss analyze -n` finds the namespace on the source paths and prints only
/// its own forms, though it analyzes the namespaces it requires.
#[test]
fn namespace_mode_prints_only_the_selected_namespace() {
    use suss_reader::edn::{Options, read_one};
    let dir = std::env::temp_dir().join(format!("suss-analyze-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("demo")).unwrap();
    std::fs::write(dir.join("demo/c.sus"), "(ns demo.c)\n(defn helper [x] (* 2 x))\n").unwrap();
    std::fs::write(dir.join("demo/b.sus"), "(ns demo.b (:require [demo.c :as c]))\n(defn g [y] (c/helper y))\n").unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_suss"))
        .args(["--no-cache", "analyze", "-n", "demo.b", "--src"])
        .arg(&dir)
        .output()
        .unwrap();
    std::fs::remove_dir_all(&dir).unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let text = String::from_utf8(output.stdout).unwrap();
    let document = read_one(&text, &Options::default()).unwrap();
    assert_eq!(asts(&document).len(), 2, "{text}");
    assert!(text.contains(":var demo.c/helper") && !text.contains(":var demo.c/helper, :name helper"), "{text}");
}
