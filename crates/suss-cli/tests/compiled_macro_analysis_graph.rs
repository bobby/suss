use suss_cli::{
    portable_macro_data::FormBridge,
    portable_macro_graph::AnalysisGraph,
    portable_session::{Session, SessionValue},
};
use suss_compile::portable::{self, Diagnostic, ExpansionContext, ExpansionHost};
use suss_reader::forms::{Form, Kind};

struct Inspect {
    macros: Session,
    bridge: FormBridge,
    query: SessionValue,
    calls: Vec<Form>,
    declaration_metadata: Option<Form>,
}
impl Inspect {
    fn new(query: &str) -> Self {
        let mut macros = Session::new_macro().unwrap();
        let bridge = FormBridge::new(&mut macros).unwrap();
        let query = macros.eval(query).unwrap();
        Self {
            macros,
            bridge,
            query,
            calls: vec![],
            declaration_metadata: None,
        }
    }
}
impl ExpansionHost for Inspect {
    fn expand(
        &mut self,
        form: &Form,
        context: ExpansionContext<'_>,
    ) -> Result<Option<Form>, Diagnostic> {
        let Kind::List(items) = &form.kind else {
            return Ok(None);
        };
        if !matches!(items.first().map(|item| &item.kind), Some(Kind::Symbol(name)) if name.namespace.is_none() && name.name == "inspect-graph")
        {
            return Ok(None);
        }
        let mut locals = context.locals.clone();
        if let Some(metadata) = &self.declaration_metadata {
            locals.get_mut("x").unwrap().declaration.metadata = vec![metadata.clone()];
        }
        let context = ExpansionContext {
            locals: &locals,
            ..context
        };
        let before = self.macros.stats();
        let env = AnalysisGraph::new(&self.bridge, &mut self.macros)
            .expansion(context)
            .map_err(|error| Diagnostic {
                span: form.span.clone(),
                message: error.to_string(),
            })?;
        self.macros.collect().unwrap();
        let result = self.macros.invoke(&self.query, &[&env]).unwrap();
        self.macros.collect().unwrap();
        self.calls.push(
            self.bridge
                .read(&mut self.macros, &result, form.span.clone())
                .unwrap(),
        );
        drop(result);
        drop(env);
        self.macros.collect().unwrap();
        assert_eq!(
            self.macros.stats().resident_fragments,
            before.resident_fragments
        );
        assert_eq!(
            self.macros.stats().resident_artifact_bytes,
            before.resident_artifact_bytes
        );
        assert_eq!(
            self.macros.stats().external_value_handles,
            before.external_value_handles
        );
        Ok(Some(Form {
            span: form.span.clone(),
            metadata: vec![],
            kind: Kind::Number(42.0),
        }))
    }
}

#[test]
fn native_analysis_graph_preserves_deep_shared_initializer_scopes_without_source_execution() {
    let query = r#"(fn [env]
      (let [binding (get (get env :locals) 'x)
            old (get binding :shadow)
            init (get binding :init)
            old-in-env (get (get (get init :env) :locals) 'x)]
        [(identical? old old-in-env)
         (loop [binding binding n 0]
           (if (get binding :shadow)
             (recur (get binding :shadow) (+ n 1)) n))
         (get init :form)
         (get (get init :suss/lowering) :suss/native-operator)]))"#;
    let mut bindings = "x (do (set! effects (+ effects 1)) 0)".to_owned();
    for _ in 0..96 {
        bindings.push_str(" x (+ x 1)");
    }
    let source = format!("(def effects 0) (def result (let [{bindings}] (inspect-graph)))");
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut host = Inspect::new(query);
        // The 96-step compiled inspection performs retained HAMT lookups.
        // Keep a bounded explicit budget for this traversal stress fixture.
        host.macros.set_operation_fuel(100_000_000);
        session.eval_with_macros(&source, &mut host).unwrap();
        assert_eq!(host.calls.len(), 1);
        let Kind::Vector(values) = &host.calls[0].kind else {
            panic!("executed graph query")
        };
        assert!(matches!(values[0].kind, Kind::Bool(true)));
        assert!(matches!(values[1].kind, Kind::Number(96.0)));
        assert!(
            matches!(&values[2].kind, Kind::List(parts) if parts.len() == 3 && matches!(&parts[0].kind, Kind::Symbol(name) if name.name == "+"))
        );
        assert!(
            matches!(&values[3].kind, Kind::String(text) if String::from_utf16(text).unwrap() == "Add")
        );
        let effects = session.eval("effects").unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        assert!(matches!(
            bridge.read(&mut session, &effects, 0..1).unwrap().kind,
            Kind::Number(1.0)
        ));
    }
}

#[test]
fn native_analysis_graph_keeps_function_declaration_environment_outside_new_self_binding() {
    let query = r#"(fn [env]
      (let [scope (nth (get env :fn-scope) 0)
            declaration-env (get scope :env)]
        [(get scope :name)
         (get (get scope :info) :suss/explicit-self)
         (get (get scope :info) :suss/phase)
         (get (get declaration-env :locals) 'self)
         (get (get env :locals) 'self)]))"#;
    let source = "(def result (let [self 7] ((fn self [] (inspect-graph)))))";
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut host = Inspect::new(query);
        session.eval_with_macros(source, &mut host).unwrap();
        let Kind::Vector(values) = &host.calls[0].kind else {
            panic!("scope query")
        };
        assert!(matches!(&values[0].kind, Kind::Symbol(name) if name.name == "self"));
        assert!(matches!(values[1].kind, Kind::Bool(true)));
        let phase = if session.phase() == portable::resolve::Phase::Runtime {
            "runtime"
        } else {
            "macro"
        };
        assert!(matches!(&values[2].kind, Kind::Keyword(name) if name.name == phase));
        for (value, expected) in [(&values[3], "let"), (&values[4], "fn")] {
            let Kind::Map(entries) = &value.kind else {
                panic!("actual binding record")
            };
            let pair = entries
                .chunks_exact(2)
                .find(|pair| matches!(&pair[0].kind, Kind::Keyword(name) if name.name == "local"))
                .unwrap();
            assert!(matches!(&pair[1].kind, Kind::Keyword(name) if name.name == expected));
        }
    }
}

#[test]
fn oversized_analysis_metadata_fails_before_runtime_effects_or_macro_store_allocation() {
    let mut session = Session::new_repl().unwrap();
    session.eval("(def effects 0)").unwrap();
    let mut host = Inspect::new("(fn [env] (count (get env :locals)))");
    let before = session.stats();
    let macro_before = host.macros.stats();
    let doc = "a".repeat(1_048_577);
    let source = format!(
        "(def bad (let [^{{:doc \"{doc}\"}} x (do (set! effects 99) 42)] (inspect-graph)))"
    );
    let error = session.eval_with_macros(&source, &mut host).unwrap_err();
    assert!(matches!(
        error,
        suss_cli::portable_session::SessionError::Compile(_)
    ));
    assert!(error.to_string().contains("UTF-16 storage bound"));
    assert!(host.calls.is_empty());
    assert_eq!(
        session.stats().resident_fragments,
        before.resident_fragments
    );
    assert_eq!(session.stats().binding_cells, before.binding_cells);
    assert_eq!(
        host.macros.stats().resident_fragments,
        macro_before.resident_fragments
    );
    assert_eq!(
        host.macros.stats().external_value_handles,
        macro_before.external_value_handles
    );
    assert!(session.eval("bad").is_err());
    let value = session.eval("(== effects 0)").unwrap();
    assert_eq!(
        session
            .inspect(&value, |store, value| Ok(value
                .unwrap_anyref()
                .unwrap()
                .as_i31(&store)?
                .unwrap()
                .get_u32()))
            .unwrap(),
        4
    );
}

#[test]
fn native_analysis_graph_preserves_unqualified_slash_keys_in_hash_maps() {
    let mut session = Session::new_repl().unwrap();
    let mut host = Inspect::new("(fn [env] (get (get (get env :locals) '/) :local))");
    session
        .eval_with_macros(
            "(let [/ 0 a 1 b 2 c 3 d 4 e 5 f 6 g 7 h 8] (inspect-graph))",
            &mut host,
        )
        .unwrap();
    assert!(matches!(&host.calls[0].kind, Kind::Keyword(name) if name.name == "let"));
}

#[test]
fn native_analysis_graph_reader_depth_is_independent_of_record_depth() {
    // Host-owned syntax at the bridge's legal limit; this is not a claim that
    // the surrounding source reader accepts equally deep enclosing syntax.
    let mut nested = Form {
        span: 0..1,
        metadata: vec![],
        kind: Kind::Number(42.0),
    };
    for _ in 0..61 {
        nested = Form {
            span: 0..1,
            metadata: vec![],
            kind: Kind::Vector(vec![nested]),
        };
    }
    let key = suss_reader::forms::read_forms(":doc").unwrap().remove(0);
    let mut host = Inspect::new(
        "(fn [env] (loop [v (get (meta (get (get (get env :locals) 'x) :name)) :doc) n 61] (if (== n 0) v (recur (nth v 0) (- n 1)))))",
    );
    host.declaration_metadata = Some(Form {
        span: 0..1,
        metadata: vec![],
        kind: Kind::Map(vec![key, nested]),
    });
    let mut session = Session::new_repl().unwrap();
    session
        .eval_with_macros("(let [x 1] (inspect-graph))", &mut host)
        .unwrap();
    assert!(matches!(host.calls[0].kind, Kind::Number(42.0)));
}

#[test]
fn native_analysis_graph_retains_staged_definition_and_function_syntax() {
    let query = r#"(fn [env]
      (let [definition (get (get (get env :ns) :defs) 'staged)
            scope (nth (get env :fn-scope) 0)]
        [(get definition :suss/initializer-recorded)
         (get definition :suss/initializer-form)
         (get (get scope :info) :suss/function-form)]))"#;
    let function = "(fn staged ([x] (inspect-graph)) ([x & xs] (inspect-graph)))";
    let expected = suss_reader::forms::read_forms(function).unwrap().remove(0);
    for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
        let mut host = Inspect::new(query);
        session.eval_with_macros(&format!("(def staged {function})"), &mut host).unwrap();
        assert_eq!(host.calls.len(), 2);
        for call in &host.calls {
            let Kind::Vector(values) = &call.kind else { panic!("staged query") };
            assert!(matches!(values[0].kind, Kind::Bool(false)));
            // Read-back spans deliberately refer to the macro call. Compare
            // actual syntax after discarding only those host-owned spans.
            fn erase_span(form: &mut Form) {
                form.span = 0..0;
                for metadata in &mut form.metadata { erase_span(metadata); }
                if let Kind::List(items) | Kind::Vector(items) = &mut form.kind {
                    for item in items { erase_span(item); }
                }
            }
            let mut expected = expected.clone(); erase_span(&mut expected);
            for actual in &values[1..] {
                let mut actual = actual.clone(); erase_span(&mut actual);
                assert_eq!(actual, expected);
            }
        }
        let result = session.eval("(== (+ (staged 1) (staged 1 2 3)) 84)").unwrap();
        assert_eq!(session.inspect(&result, |store, value| Ok(value.unwrap_anyref().unwrap().as_i31(&store)?.unwrap().get_u32())).unwrap(), 4);
    }
}
