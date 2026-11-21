//! Runtime reader throughput (#300): `read-string` and `suss.edn/read-string`
//! over the analyzer-shaped document of suss-reader's `edn_bench`, with
//! `pr-str` of the same data for reference. Run in the profile being
//! measured, for example
//! `cargo run --release -p suss-cli --example read_string_bench -- 200 [--print]`.
use std::collections::BTreeMap;
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


// Measurement-only output capture; node() above is the unchanged benchmark fixture.
fn main() {
    let roots: usize = std::env::args().nth(1).unwrap().parse().unwrap();
    let output = std::env::args().nth(2).unwrap();
    let document = Value::Vector((0..roots).map(|i| node(i, 3)).collect());
    let text = edn::print(&document).unwrap();
    let mut session = Session::new_repl_with_options(SessionOptions {
        source_paths: Vec::new(), fuel_per_operation: u64::MAX / 4,
    }).unwrap();
    let literal = text.replace('\\', "\\\\").replace('"', "\\\"");
    session.eval(&format!("(def text \"{literal}\")")).unwrap();
    session.eval("(def readers {'suss/binding identity})").unwrap();
    session.eval("(def value (binding [*data-readers* readers] (read-string text)))").unwrap();
    let printed = session.eval("(pr-str value)").unwrap();
    session.collect().unwrap();
    let bridge = suss_cli::portable_macro_data::FormBridge::new(&mut session).unwrap();
    let form = bridge.read(&mut session, &printed, 0..0).unwrap();
    let suss_reader::forms::Kind::String(units) = &form.kind else { panic!("printer result") };
    let bytes = units.iter().flat_map(|unit| unit.to_le_bytes()).collect::<Vec<_>>();
    std::fs::write(output, bytes).unwrap();
    println!("{roots} roots, input {} UTF-16 units, output {} UTF-16 units", text.encode_utf16().count(), units.len());
}
