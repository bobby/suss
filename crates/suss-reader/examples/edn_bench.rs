//! EDN reader and printer throughput over a generated document shaped like
//! analyzer output (#299): maps of keywords, symbols, strings, numbers and
//! nested child vectors. Run in the profile being measured, for example
//! `cargo run --release -p suss-reader --example edn_bench -- 20000`.
use std::collections::BTreeMap;
use std::time::Instant;
use suss_reader::edn::{self, Options, Value};
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

fn best<T>(runs: usize, mut f: impl FnMut() -> T) -> (f64, T) {
    let mut best = f64::INFINITY;
    let mut result = None;
    for _ in 0..runs {
        let start = Instant::now();
        let value = f();
        best = best.min(start.elapsed().as_secs_f64());
        result = Some(value);
    }
    (best, result.unwrap())
}

fn main() {
    let roots: usize = std::env::args().nth(1).map_or(2000, |n| n.parse().unwrap());
    let document = Value::Vector((0..roots).map(|i| node(i, 3)).collect());
    let (print_time, text) = best(3, || edn::print(&document).unwrap());
    let options = Options { work_allowance: u64::MAX, ..Options::preserving_tags() };
    let (read_time, back) = best(3, || edn::read_one(&text, &options).unwrap());
    assert!(back == document);
    let (forms_time, _) = best(3, || suss_reader::forms::read_forms_with_work_allowance(&text.replace("#suss/binding ", ""), u64::MAX).unwrap());
    println!("forms reader alone (code mode, tags removed): {forms_time:.3} s");
    let megabytes = text.len() as f64 / 1e6;
    println!(
        "{roots} roots, {:.2} MB: print {:.3} s ({:.1} MB/s), read {:.3} s ({:.1} MB/s)",
        megabytes,
        print_time,
        megabytes / print_time,
        read_time,
        megabytes / read_time,
    );
}
