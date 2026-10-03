//! Native declaration preservation; primary equality is asserted independently.
use suss_cli::{
    portable_macro_data::FormBridge,
    portable_macros::CompiledMacros,
    portable_session::{Session, SessionOptions},
};
use suss_compile::portable::resolve::Phase;
use suss_reader::forms::{Form, Kind};

const SNAPSHOT: &str = r#"(defmacro declaration-snapshot [name]
  (let [fields [:name :ns :private :dynamic :doc :declared :tag :ret-tag
                :fn-var :variadic? :max-fixed-arity :method-params
                :arglists :arglists-meta :meta :line :column :file]
        groups [(get &env :ns) (get &env :suss/catalog)]]
    (list 'quote
      (loop [g 0 rows []]
        (if (< g (count groups))
          (let [defs (get (nth groups g) :defs) info (get defs name)]
            (recur (+ g 1)
              (conj rows
                [(contains? defs name)
                 (loop [i 0 properties []]
                   (if (< i (count fields))
                     (let [key (nth fields i)]
                       (recur (+ i 1) (conj properties
                         [key (contains? info key) (get info key)])))
                     properties))
                 (contains? info :suss/source-function)])))
          rows)))))"#;

fn normalize(form: &mut Form) {
    form.span = 0..0;
    form.metadata.clear();
    match &mut form.kind {
        Kind::List(items) | Kind::Vector(items) | Kind::Map(items) | Kind::Set(items) => {
            for item in items {
                normalize(item);
            }
        }
        _ => {}
    }
}
fn read(session: &mut Session, value: &suss_cli::portable_session::SessionValue) -> Form {
    let bridge = FormBridge::new(session).unwrap();
    session.collect().unwrap();
    let mut data = bridge.read(session, value, 0..1).unwrap();
    normalize(&mut data);
    data
}
fn snapshot(session: &mut Session, macros: &mut CompiledMacros, name: &str) -> Form {
    let value = session
        .eval_with_macros(&format!("(declaration-snapshot {name})"), macros)
        .unwrap();
    let data = read(session, &value);
    let Kind::Vector(groups) = &data.kind else {
        panic!("Declaration groups")
    };
    assert_eq!(groups.len(), 2);
    for group in groups {
        let Kind::Vector(items) = &group.kind else {
            panic!("Declaration group")
        };
        assert_eq!(items.len(), 3);
        assert_eq!(items[0].kind, Kind::Bool(true));
        let Kind::Vector(fields) = &items[1].kind else {
            panic!("Declaration fields")
        };
        assert_eq!(fields.len(), 18);
    }
    data
}
fn assert_property(snapshot: &Form, key: &str, expected: Kind) {
    assert_selected_property(snapshot, key, true, expected);
}
fn assert_selected_property(snapshot: &Form, key: &str, present: bool, expected: Kind) {
    let Kind::Vector(groups) = &snapshot.kind else {
        unreachable!()
    };
    for group in groups {
        let Kind::Vector(items) = &group.kind else {
            unreachable!()
        };
        let Kind::Vector(fields) = &items[1].kind else {
            unreachable!()
        };
        let property = fields
            .iter()
            .find_map(|field| {
                let Kind::Vector(row) = &field.kind else {
                    panic!("Property row")
                };
                assert_eq!(row.len(), 3);
                matches!(&row[0].kind, Kind::Keyword(name) if name.name == key).then_some(row)
            })
            .expect("Selected property");
        assert_eq!(property[1].kind, Kind::Bool(present), "Present {key}");
        assert_eq!(property[2].kind, expected, "Value {key}");
    }
}

#[test]
fn repeated_declare_keeps_existing_snapshot_catalog_and_runtime_bindings() {
    let mut differences = Vec::new();
    for phase in [Phase::Runtime, Phase::Macro] {
        let mut session = Session::with_options_in(SessionOptions::default(), phase).unwrap();
        session.set_operation_fuel(100_000_000);
        session
            .eval(include_str!("../../../runtime/core-import/suss/core.sus"))
            .unwrap();
        session.enter_namespace("declaration-review").unwrap();
        let mut macros = CompiledMacros::new().unwrap();
        macros.set_operation_fuel(100_000_000);
        macros.enter_namespace("declaration-review").unwrap();
        macros.define(SNAPSHOT).unwrap();
        session
            .eval(
                r#"
          (def effects 0)
          (def ^:private scalar "scalar-doc" (do (set! effects (+ effects 1)) 7))
          (def ^:private callable "callable-doc" (fn [x] (+ x 1)))
          (def old-callable callable)
        "#,
            )
            .unwrap();
        let scalar = snapshot(&mut session, &mut macros, "scalar");
        let callable = snapshot(&mut session, &mut macros, "callable");
        assert_property(&scalar, "private", Kind::Bool(true));
        assert_property(
            &scalar,
            "doc",
            Kind::String("scalar-doc".encode_utf16().collect()),
        );
        assert_property(&callable, "fn-var", Kind::Bool(true));
        assert_property(
            &callable,
            "doc",
            Kind::String("callable-doc".encode_utf16().collect()),
        );
        session
            .eval(
                r#"(declare forward
          ^{:private false :doc "replacement" :tag string} scalar
          ^{:private false :doc "replacement" :fn-var false} callable)"#,
            )
            .unwrap();
        for (name, before) in [("scalar", &scalar), ("callable", &callable)] {
            let after = snapshot(&mut session, &mut macros, name);
            if &after != before {
                differences.push(format!(
                    "{phase:?} completed {name}: {before:?} versus {after:?}"
                ));
            }
        }
        session
            .eval("(def ^{:declared :marker :doc \"ignored\"} callable)")
            .unwrap();
        let after_truthy = snapshot(&mut session, &mut macros, "callable");
        if after_truthy != callable {
            differences.push(format!("{phase:?} truthy direct def changed callable"));
        }
        session
            .eval("(declare ^{:declared false :doc \"ignored\"} callable)")
            .unwrap();
        if snapshot(&mut session, &mut macros, "callable") != callable {
            differences.push(format!(
                "{phase:?} declare must override false declared metadata"
            ));
        }
        assert!(session.eval("(declare other.namespace/callable)").is_err());
        assert!(session.eval("(declare 1)").is_err());
        assert_eq!(
            snapshot(&mut session, &mut macros, "callable"),
            after_truthy
        );
        let forward = snapshot(&mut session, &mut macros, "forward");
        assert_property(&forward, "declared", Kind::Bool(true));
        session
            .eval(r#"(declare ^{:doc "changed-forward" :tag number} forward)"#)
            .unwrap();
        let after = snapshot(&mut session, &mut macros, "forward");
        if forward != after {
            differences.push(format!("{phase:?} forward: {forward:?} versus {after:?}"));
        }
        for (source, expected) in [
            ("scalar", 7.0),
            ("(callable 20)", 21.0),
            ("(old-callable 22)", 23.0),
            ("effects", 1.0),
            ("(def forward 19) forward", 19.0),
        ] {
            let value = session.eval(source).unwrap();
            assert_eq!(
                read(&mut session, &value).kind,
                Kind::Number(expected),
                "{phase:?}: {source}"
            );
        }
        session
            .eval(
                r#"(def ordinary "old" 1) (def ordinary)
            (def false-record "old" 1)
            (def ^{:declared false :doc "new"} false-record)
            (def nil-record "old" 1)
            (def ^{:declared nil :doc "new"} nil-record)
            (def initialized "old" 1)
            (def ^{:declared :marker} initialized "new" 9)"#,
            )
            .unwrap();
        assert_selected_property(
            &snapshot(&mut session, &mut macros, "ordinary"),
            "doc",
            false,
            Kind::Nil,
        );
        assert_property(
            &snapshot(&mut session, &mut macros, "false-record"),
            "doc",
            Kind::String("new".encode_utf16().collect()),
        );
        assert_property(
            &snapshot(&mut session, &mut macros, "false-record"),
            "declared",
            Kind::Bool(false),
        );
        assert_property(
            &snapshot(&mut session, &mut macros, "nil-record"),
            "doc",
            Kind::String("new".encode_utf16().collect()),
        );
        assert_property(
            &snapshot(&mut session, &mut macros, "nil-record"),
            "declared",
            Kind::Nil,
        );
        assert_selected_property(
            &snapshot(&mut session, &mut macros, "initialized"),
            "doc",
            false,
            Kind::Nil,
        );
        let value = session.eval("initialized").unwrap();
        assert_eq!(read(&mut session, &value).kind, Kind::Number(9.0));
    }
    assert!(differences.is_empty(), "{}", differences.join("\n"));
}
