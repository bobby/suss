#[path = "../../suss-compile/tests/support/probe_eval.rs"]
mod probe_eval;
use probe_eval::ProbeEval;
use suss_cli::portable_session::{Session, SessionValue};

fn units(session: &mut Session, value: &SessionValue) -> Vec<u16> {
    session
        .inspect(value, |mut store, value| {
            let text = value.unwrap_anyref().unwrap().as_array(&store)?.unwrap();
            Ok(text
                .elems(&mut store)?
                .map(|v| v.unwrap_i32() as u16)
                .collect())
        })
        .unwrap()
}
#[test]
fn compiled_macro_prerequisite_quotes_keep_real_identifiers_and_lists_through_gc() {
    let mut session = Session::new_repl().unwrap();
    session
        .eval("(def saved '(hello :app/world (nested \"\\uD800😀\")))")
        .unwrap();
    session.collect().unwrap();
    let name = session.eval("(name (first saved))").unwrap();
    assert_eq!(
        units(&mut session, &name),
        "hello".encode_utf16().collect::<Vec<_>>()
    );
    let keyword = session.eval("(namespace (first (rest saved)))").unwrap();
    assert_eq!(
        units(&mut session, &keyword),
        "app".encode_utf16().collect::<Vec<_>>()
    );
    let string = session
        .eval("(first (rest (first (rest (rest saved)))))")
        .unwrap();
    assert_eq!(units(&mut session, &string), vec![0xd800, 0xd83d, 0xde00]);
}

fn class_id(session: &mut Session, name: &str) -> i64 {
    let value = session.eval(&format!("suss.core/{name}")).unwrap();
    session.collect().unwrap();
    session
        .inspect(&value, |mut store, value| {
            let constructor = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
            let fields = constructor.fields(&mut store)?.collect::<Vec<_>>();
            assert_eq!(fields.len(), 5, "Class closure layout");
            let wrapper = fields[0]
                .unwrap_anyref()
                .unwrap()
                .as_struct(&store)?
                .unwrap();
            assert_eq!(
                wrapper.fields(&mut store)?.count(),
                4,
                "closure property owner layout"
            );
            let storage = wrapper
                .field(&mut store, 1)?
                .unwrap_anyref()
                .unwrap()
                .as_array(&store)?
                .unwrap();
            assert_eq!(
                storage.len(&store)?,
                3,
                "original environment, property entries and source display name"
            );
            let properties = storage
                .get(&mut store, 1)?
                .unwrap_anyref()
                .unwrap()
                .as_array(&store)?
                .expect("closure property entries are an array");
            assert_eq!(properties.len(&store)? % 2, 0, "key/value property entries");
            // Type constructors are kernel closures, not named source functions.
            // Their unavailable display name remains nil; never interpret it as
            // an anonymous source function or as part of the captured descriptor.
            let source_name_value = storage.get(&mut store, 2)?;
            assert_eq!(
                suss_cli::portable_session::sentinel_tag(&mut store, &source_name_value)?
                    .expect("unset constructor source name"),
                0,
                "constructor source display name is explicitly unavailable"
            );
            let descriptor = storage
                .get(&mut store, 0)?
                .unwrap_anyref()
                .unwrap()
                .as_struct(&store)?
                .unwrap();
            assert_eq!(
                descriptor.fields(&mut store)?.count(),
                6,
                "Descriptor layout"
            );
            Ok(descriptor.field(&mut store, 0)?.unwrap_i64())
        })
        .unwrap()
}
fn decode(
    store: &mut wasmtime::StoreContextMut<'_, ()>,
    value: &wasmtime::Val,
    classes: &std::collections::BTreeMap<i64, &str>,
    depth: usize,
) -> wasmtime::Result<serde_json::Value> {
    use serde_json::json;
    assert!(depth < 64, "bounded native observation");
    if let Some(sentinel) = suss_cli::portable_session::sentinel_tag(&mut *store, value)? {
        return Ok(match sentinel {
            0 => json!({"tag":"nil"}),
            2 => json!({"tag":"bool","value":false}),
            4 => json!({"tag":"bool","value":true}),
            other => panic!("unknown sentinel {other}"),
        });
    }
    match suss_cli::portable_session::runtime_number(&mut *store, value)? {
        Some(suss_cli::portable_session::RuntimeNumber::Long(value)) => {
            return Ok(json!({"tag":"long","value":value}));
        }
        Some(suss_cli::portable_session::RuntimeNumber::Double(value)) => {
            return Ok(json!({"tag":"f64","bits":format!("{:016x}", value.to_bits())}));
        }
        None => {}
    }
    let value = value.unwrap_anyref().unwrap();
    if let Some(text) = value.as_array(&*store)? {
        assert!(matches!(
            text.ty(&*store)?.element_type(),
            wasmtime::StorageType::I16
        ));
        return Ok(
            json!({"tag":"string","units":text.elems(&mut *store)?.map(|v|v.unwrap_i32()as u16).collect::<Vec<_>>()}),
        );
    }
    let object = value.as_struct(&*store)?.expect("known portable object");
    let fields = object.fields(&mut *store)?.collect::<Vec<_>>();
    assert_eq!(fields.len(), 4, "ABI2 Object layout");
    let descriptor = fields[0]
        .unwrap_anyref()
        .unwrap()
        .as_struct(&*store)?
        .unwrap();
    let id = descriptor.field(&mut *store, 0)?.unwrap_i64();
    let kind = classes
        .get(&id)
        .unwrap_or_else(|| panic!("unknown nominal descriptor {id}"));
    let data = fields[1]
        .unwrap_anyref()
        .unwrap()
        .as_array(&*store)?
        .unwrap();
    let data = data.elems(&mut *store)?.collect::<Vec<_>>();
    match *kind {
        "Symbol" | "Keyword" => {
            assert_eq!(data.len(), if *kind == "Symbol" { 5 } else { 4 });
            Ok(json!({"tag":if *kind=="Symbol" {"symbol"}else{"keyword"},
                "namespace":decode(store,&data[0],classes,depth+1)?,
                "name":decode(store,&data[1],classes,depth+1)?}))
        }
        "EmptyList" => {
            assert_eq!(data.len(), 1);
            Ok(json!({"tag":"seq","items":[]}))
        }
        "List" => {
            assert_eq!(data.len(), 5);
            let first = decode(store, &data[1], classes, depth + 1)?;
            let tail = decode(store, &data[2], classes, depth + 1)?;
            let mut items = vec![first];
            match tail["tag"].as_str().unwrap() {
                "nil" => {}
                "seq" => items.extend(tail["items"].as_array().unwrap().iter().cloned()),
                tag => panic!("unexpected List tail {tag}"),
            }
            let count = decode(store, &data[3], classes, depth + 1)?;
            assert_eq!(
                count,
                json!({"tag":"long","value":items.len()})
            );
            Ok(json!({"tag":"seq","items":items}))
        }
        other => panic!("unsupported nominal observation {other}"),
    }
}
/// `expected` with each integral ClojureScript double observation as the long
/// JVM Clojure reads for it.
fn clojure_numbers(expected: &serde_json::Value) -> serde_json::Value {
    match expected {
        serde_json::Value::Object(fields) if fields.get("tag").is_some_and(|tag| tag == "f64") => {
            let bits = u64::from_str_radix(fields["bits"].as_str().unwrap(), 16).unwrap();
            let value = f64::from_bits(bits);
            // `1e21` and the like stay doubles: Clojure reads them as such.
            if value.fract() == 0.0 && value.abs() < 2f64.powi(53) && bits != (-0.0f64).to_bits() {
                serde_json::json!({"tag":"long", "value":value as i64})
            } else {
                expected.clone()
            }
        }
        serde_json::Value::Object(fields) => serde_json::Value::Object(
            fields.iter().map(|(key, value)| (key.clone(), clojure_numbers(value))).collect(),
        ),
        serde_json::Value::Array(items) => items.iter().map(clojure_numbers).collect(),
        other => other.clone(),
    }
}
#[test]
fn compiled_macro_prerequisite_quotes_match_lossless_primary_observations() {
    let corpus: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/oracle/quoted-identifier-cases.json"
    ))
    .unwrap();
    let mut session = Session::new_repl().unwrap();
    let classes = ["Symbol", "Keyword", "List", "EmptyList"]
        .into_iter()
        .map(|name| (class_id(&mut session, name), name))
        .collect::<std::collections::BTreeMap<_, _>>();
    let mut ids = std::collections::BTreeSet::new();
    for case in corpus["cases"].as_array().unwrap() {
        assert!(ids.insert(case["id"].as_str().unwrap()));
        let source = case["source"].as_str().unwrap();
        let value = session
            .eval_probe(source)
            .unwrap_or_else(|error| panic!("{source}: {error}"));
        session.collect().unwrap();
        let actual = session
            .inspect(&value, |mut store, value| {
                decode(&mut store, &value, &classes, 0)
            })
            .unwrap();
        assert_eq!(actual, clojure_numbers(&case["expected"]), "{source}");
    }
    assert_eq!(ids.len(), 48);
}

#[test]
fn compiled_macro_prerequisite_quote_errors_are_located_and_preserve_session_state() {
    use suss_cli::portable_session::SessionError;
    let mut session = Session::new_repl().unwrap();
    session
        .eval("(def keep 'app/original) (def effects 0)")
        .unwrap();
    for source in [
        "(def leaked 99) (quote)",
        "(def leaked 99) (quote x y)",
        "(def leaked 99) (def keep '^42 replacement)",
        "(def leaked 99) (def keep '^42 [1 2])",
        "(def leaked 99) (def keep '^42 {:x 1})",
        "(def leaked 99) (def keep '^42 #{1 2})",
    ] {
        let before = session.stats();
        let Err(SessionError::Compile(error)) = session.eval(source) else {
            panic!("located quote failure: {source}");
        };
        assert!(error.span.start < error.span.end);
        assert_eq!(session.stats(), before);
        assert!(matches!(
            session.eval("leaked"),
            Err(SessionError::Compile(_))
        ));
        let kept = session.eval("(name keep)").unwrap();
        assert_eq!(
            units(&mut session, &kept),
            "original".encode_utf16().collect::<Vec<_>>()
        );
    }
    // Quoted code is data, including invalid callee names and throwing forms.
    session
        .eval("(def data '(do (set! effects 99) (throw 31) unresolved))")
        .unwrap();
    let effects = session.eval("effects").unwrap();
    session
        .inspect(&effects, |store, value| {
            assert_eq!(
                suss_cli::portable_session::runtime_number(store, &value)?,
                Some(suss_cli::portable_session::RuntimeNumber::Long(0))
            );
            Ok(())
        })
        .unwrap();
    let retained = session.eval("(first data)").unwrap();
    session.eval("(def data nil) (def keep nil)").unwrap();
    session.collect().unwrap();
    let classes = ["Symbol", "Keyword", "List", "EmptyList"]
        .into_iter()
        .map(|name| (class_id(&mut session, name), name))
        .collect();
    let actual = session
        .inspect(&retained, |mut store, value| {
            decode(&mut store, &value, &classes, 0)
        })
        .unwrap();
    assert_eq!(
        actual,
        serde_json::json!({"tag":"symbol","namespace":{"tag":"nil"},"name":{"tag":"string","units":[100,111]}})
    );
}
