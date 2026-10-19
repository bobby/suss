use super::*;
use crate::portable_session::memo_leaf_test_support::Method;

fn actual_body(text: &str) -> Arc<hir::SourceAnalysis> {
    let forms = suss_reader::forms::read_forms(text).unwrap();
    let root = hir::analyze(&forms, 0..text.len()).unwrap();
    fn source_root(node: &Hir) -> Option<&Arc<hir::SourceAnalysis>> {
        if let Some(source) = &node.source
            && (matches!(source.node.as_deref(), Some(hir::SourceNode::Bindings { .. } | hir::SourceNode::Future(_)))
                || source.callable.is_some())
        {
            return Some(source);
        }
        match &node.kind {
            Expression::Do(items) => items.iter().rev().find_map(source_root),
            _ => None,
        }
    }
    // Callable key initializers may introduce extra native Do wrappers.
    let source = source_root(&root).expect("actual user source root");
    let body = match source.node.as_deref() {
        Some(hir::SourceNode::Bindings { body, .. }) | Some(hir::SourceNode::Future(body)) => body.source.as_ref().unwrap(),
        _ => source.callable.as_ref().unwrap().methods[0].body.source.as_ref().unwrap(),
    };
    assert!(body.is_body);
    body.clone()
}

fn memo(session: &mut Session, bridge: &FormBridge, value: &SessionValue) -> Kind {
    let fields = session.data_fields(value).unwrap();
    bridge.read(session, &fields[3], 0..0).unwrap().kind
}

#[test]
fn analyzer_literal_body_key_requires_actual_origin_and_preserves_live_hash_after_gc() {
    let binding = actual_body("(let* [x 17] x)");
    let method = actual_body("(fn* [] 17)");
    let future = actual_body("(suss.async/future* 17)");
    assert_eq!(binding.body_origin, Some(hir::SourceBodyOrigin::Bindings));
    assert_eq!(method.body_origin, Some(hir::SourceBodyOrigin::FunctionMethod));
    assert_eq!(future.body_origin, Some(hir::SourceBodyOrigin::SussFuture));
    // An otherwise genuine body with absent provenance must stay unknown/cold.
    let mut unknown = binding.as_ref().clone();
    unknown.body_origin = None;
    let unknown = Arc::new(unknown);
    for mut session in [Session::new_macro().unwrap(), Session::new_repl().unwrap()] {
        session.eval("(def leaf-original-method nil) (def body-origin-events [])").unwrap();
        let original = Method::capture(&mut session, "Keyword").unwrap();
        original.bind_delegate(&mut session).unwrap();
        session.eval(r#"(extend-type Keyword IHash
          (-hash [self]
            (let [memo (.-_hash self)]
              (do (if (= (.-name self) "body?")
                    (set! body-origin-events (conj body-origin-events memo)) nil)
                  (if (nil? memo) (leaf-original-method self) memo)))))"#).unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        for (label, source, expected) in [
            ("binding", &binding, Kind::Number(-1333761917.0)),
            ("method", &method, Kind::Number(-1333761917.0)),
            ("future", &future, Kind::Nil),
            ("unknown", &unknown, Kind::Nil),
        ] {
            session.eval("(set! body-origin-events [])").unwrap();
            let operation = session.operation_budget_scope().unwrap();
            let value = {
                let mut graph = AnalysisGraph::new(&bridge, &mut session);
                let key = graph.source_body_key(source).unwrap();
                let key = graph.keyword(key).unwrap();
                graph.materialize(key).unwrap()
            };
            drop(operation);
            session.collect().unwrap();
            assert_eq!(memo(&mut session, &bridge, &value), expected, "{label} before IHash");
            let hash = session.eval("hash").unwrap();
            let result = session.invoke(&hash, &[&value]).unwrap();
            assert_eq!(bridge.read(&mut session, &result, 0..0).unwrap().kind, Kind::Number(-1333761917.0));
            let events = session.eval("body-origin-events").unwrap();
            let Kind::Vector(events) = bridge.read(&mut session, &events, 0..0).unwrap().kind else { panic!("event vector") };
            assert_eq!(events.len(), 1, "{label} exact live IHash dispatch");
            assert_eq!(events[0].kind, expected, "{label} memo before delegate");
        }
        original.restore(&mut session).unwrap();
        session.eval("(set! body-origin-events [])").unwrap();
        let value = {
            let mut graph = AnalysisGraph::new(&bridge, &mut session);
            let key = graph.source_body_key(&binding).unwrap();
            let key = graph.keyword(key).unwrap();
            graph.materialize(key).unwrap()
        };
        session.collect().unwrap();
        let hash = session.eval("hash").unwrap();
        let result = session.invoke(&hash, &[&value]).unwrap();
        assert_eq!(bridge.read(&mut session, &result, 0..0).unwrap().kind, Kind::Number(-1333761917.0));
        let events = session.eval("body-origin-events").unwrap();
        assert!(matches!(bridge.read(&mut session, &events, 0..0).unwrap().kind, Kind::Vector(ref events) if events.is_empty()));
    }
}
