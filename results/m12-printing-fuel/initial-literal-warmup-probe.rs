//! Development-only observation of the unchanged public printer invocation.
use suss_compile::portable_session::Session;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let budget: u64 = args[1].parse().unwrap();
    let replacement = args.get(2).map(|path| std::fs::read_to_string(path).unwrap());
    let pattern = [97, 34, 92, 10, 0x03bb, 0xd800, 120, 0xdc00];
    let escaped = [97, 92, 34, 92, 92, 92, 110, 0x03bb, 0xd800, 120, 0xdc00];
    for (phase, mut session) in [("Runtime", Session::new_repl().unwrap()), ("Macro", Session::new_macro().unwrap())] {
        session.set_operation_fuel(budget);
        if let Some(source) = &replacement {
            session.eval_privileged(&format!("(ns suss.core) {source}")).unwrap();
        }
        let printer = session.eval("pr-str").unwrap();
        for count in [0, 1, 16, 512, 4096] {
            let source = format!("\"{}\"", "a\\\"\\\\\\nλ\\uD800x\\uDC00".repeat(count));
            let input = session.eval(&source).unwrap();
            session.inspect(&input, |mut store, value| {
                let array = value.unwrap_anyref().unwrap().as_array(&store)?.unwrap();
                let units = array.elems(&mut store)?.map(|v| v.unwrap_i32() as u16).collect::<Vec<_>>();
                assert_eq!(units, pattern.repeat(count));
                Ok(())
            }).unwrap();
            let result = match session.invoke(&printer, &[&input]) {
                Ok(value) => value,
                Err(error) => {
                    println!("{phase} count={count} budget={budget} ERROR {error}");
                    continue;
                }
            };
            let remaining = session.inspect(&result, |store, _| store.get_fuel()).unwrap();
            session.collect().unwrap();
            session.inspect(&result, |mut store, value| {
                let array = value.unwrap_anyref().unwrap().as_array(&store)?.unwrap();
                let units = array.elems(&mut store)?.map(|v| v.unwrap_i32() as u16).collect::<Vec<_>>();
                let mut expected = vec![34];
                expected.extend(escaped.repeat(count));
                expected.push(34);
                assert_eq!(units, expected);
                Ok(())
            }).unwrap();
            println!("{phase} count={count} budget={budget} consumed={}", budget - remaining);
        }
    }
}
