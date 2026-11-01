//! Runtime reader throughput (#300): `read-string` and `suss.edn/read-string`
//! over the analyzer-shaped document of suss-reader's `edn_bench`, with
//! `pr-str` of the same data for reference. Run in the profile being
//! measured, for example
//! `cargo run --release -p suss-cli --example read_string_bench -- 200 [--print]`.
use std::collections::BTreeMap;
use std::time::Instant;
use suss_cli::portable_session::{Session, SessionOptions};
use suss_reader::edn::{self, Value};
use suss_reader::Symbol;

fn node(id: usize, depth: usize) -> Value {
    let keyword = Value::keyword;
    let mut map = BTreeMap::from([
        (keyword("op"), keyword(["invoke", "local", "const", "do", "if"][id % 5])),
        (keyword("id"), Value::Number(id as f64)),
        (keyword("form"), Value::Symbol(Symbol::namespaced("suss.core", format!("f{}", id % 97)))),
        (keyword("env"), Value::Map(BTreeMap::from([
            (keyword("ns"), Value::Symbol(Symbol::new("suss.core"))),
            (keyword("context"), keyword("expr")),
            (keyword("span"), Value::Vector(vec![Value::Number((id * 13) as f64), Value::Number((id * 13 + 7) as f64)])),
        ]))),
        (keyword("doc"), Value::String(format!("node {id} with \"quotes\" and é"))),
    ]);
    if depth > 0 {
        let children: Vec<Value> = (0..3).map(|i| node(id * 3 + i, depth - 1)).collect();
        map.insert(keyword("children"), Value::Vector(vec![keyword("args")]));
        map.insert(keyword("args"), Value::Vector(children));
        map.insert(keyword("binding"), Value::Tagged(Symbol::namespaced("suss", "binding"), Box::new(Value::Number(id as f64))));
    }
    Value::Map(map)
}

/// The best of `runs` timings of evaluating `source`, in seconds.
fn best(session: &mut Session, runs: usize, source: &str) -> f64 {
    (0..runs)
        .map(|_| {
            let start = Instant::now();
            session.eval(source).unwrap_or_else(|error| panic!("{source}: {error:?}"));
            start.elapsed().as_secs_f64()
        })
        .fold(f64::INFINITY, f64::min)
}

fn main() {
    let roots: usize = std::env::args().nth(1).map_or(200, |n| n.parse().unwrap());
    let document = Value::Vector((0..roots).map(|i| node(i, 3)).collect());
    let text = edn::print(&document).unwrap();
    let units = text.encode_utf16().count() as f64;
    let mut session = Session::new_repl_with_options(SessionOptions {
        source_paths: Vec::new(),
        fuel_per_operation: u64::MAX / 4,
    })
    .unwrap();
    let literal = text.replace('\\', "\\\\").replace('"', "\\\"");
    session.eval(&format!("(def text \"{literal}\")")).unwrap();
    session.eval("(def readers {'suss/binding identity})").unwrap();
    let baseline = best(&mut session, 5, "(count text)");
    let code = best(&mut session, 3, "(count (binding [*data-readers* readers] (read-string text)))") - baseline;
    let data = best(&mut session, 3, "(count (suss.core/read-edn-string {:readers readers} text))") - baseline;
    // pr-str, for reference, only when asked: it grows faster than linearly.
    let printed = if std::env::args().any(|argument| argument == "--print") {
        session.eval("(def value (binding [*data-readers* readers] (read-string text)))").unwrap();
        best(&mut session, 3, "(count (pr-str value))") - baseline
    } else {
        f64::NAN
    };
    println!("{roots} roots, {:.3} M UTF-16 units ({:.2} MB)", units / 1e6, text.len() as f64 / 1e6);
    for (label, time) in [("read-string", code), ("suss.edn/read-string", data), ("pr-str", printed)] {
        println!("{label}: {time:.3} s, {:.3} M units/s", units / time / 1e6);
    }
}
