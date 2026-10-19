//! Regression gate for source-controller callback order. UNEXECUTED.
//! The comparator is the existing single native walk, NOT an executed pinned-JS
//! oracle. Upstream vector emission supports left-to-right member evaluation;
//! this test does not claim an upstream constructor callback multiplicity.
use super::*;
use crate::portable::{
    hir::{self, Hir, Expression},
    resolve::{Environment, Phase},
};
use suss_reader::forms::Kind;
use crate::portable_macro_data::FormBridge;
use crate::portable_session::memo_leaf_test_support::Method;

#[derive(Clone, Copy, Debug)]
enum Route {
    SingleWalk,
    WholeController,
}
#[derive(Clone, Copy, Debug)]
enum Delegate {
    OriginalEveryTime,
    NilAware,
}
struct Observation {
    trace: String,
    shape: String,
}
fn observed(
    phase: Phase,
    route: Route,
    delegate: Delegate,
) -> std::result::Result<Observation, String> {
    // This is the compiler-data receiver (Macro store) for either caller-HIR
    // phase, matching environment transport. Public paired-caller tests remain
    // mandatory; this source-level receiver fixture does not replace them.
    let source = "(def ^{:order.meta/k0 0 :order.meta/k1 1 :order.meta/k2 2 :order.meta/k3 3 :order.meta/k4 4 :order.meta/k5 5 :order.meta/k6 6 :order.meta/k7 7 :order.meta/k8 8 :order.meta/k9 9} watched 17)";
    let forms = suss_reader::forms::read_forms(source).map_err(|e| e.to_string())?;
    let environment = hir::prepare(&forms, 0..source.len(), &Environment::default(), phase)
        .map_err(|e| e.to_string())?
        .1;
    let binding = environment
        .resolve(phase, &suss_reader::Symbol::new("watched"), 0..0)
        .map_err(|e| e.to_string())?;
    let global = binding.global().clone();
    let info = environment.shared_definition_info(&global).unwrap().clone();
    // A genuine HIR-produced completed source; no fabricated owner/certificate,
    // fake authority, guest spelling join or arbitrary RecordedMetadata object.
    let owner = info
        .completed_values
        .as_ref()
        .ok_or("missing genuine ordinary source owner")?;
    let mut session = Session::new_macro().map_err(|e| e.to_string())?;
    let bridge = FormBridge::new(&mut session).map_err(|e| e.to_string())?;
    session
        .eval("(def leaf-original-method nil) (def source-order-events [])")
        .map_err(|e| e.to_string())?;
    let original = Method::capture(&mut session, "Keyword").map_err(|e| e.to_string())?;
    original
        .bind_delegate(&mut session)
        .map_err(|e| e.to_string())?;
    let tail = match delegate {
        Delegate::OriginalEveryTime => "(leaf-original-method self)",
        Delegate::NilAware => "(if (nil? memo) (leaf-original-method self) memo)",
    };
    let code = format!(
        r#"(extend-type Keyword IHash (-hash [self]
      (let [memo (.-_hash self)]
        (do (if (or (= (.-ns self) "order.inline") (= (.-ns self) "order.meta"))
          (set! source-order-events (conj source-order-events [(.-ns self) (.-name self) memo])) nil)
          {tail}))))"#
    );
    let result = (|| -> std::result::Result<Observation, String> {
        session.eval(&code).map_err(|e| e.to_string())?;
        // Installation/compilation must not count as transport observations.
        session
            .eval("(set! source-order-events [])")
            .map_err(|e| e.to_string())?;
        let result = {
            let mut graph = AnalysisGraph::new(&bridge, &mut session);
            graph.capture_roots = true;
            graph.direct_reader_roots = true;
            let inline_form = suss_reader::forms::read_forms("{:order.inline/k0 0 :order.inline/k1 1 :order.inline/k2 2 :order.inline/k3 3 :order.inline/k4 4 :order.inline/k5 5 :order.inline/k6 6 :order.inline/k7 7 :order.inline/k8 8 :order.inline/k9 9}")
                .map_err(|e|e.to_string())?.remove(0);
            let inline = graph.form(&inline_form, 0).map_err(|e| e.to_string())?;
            // Only this second member has a genuine completed-source boundary.
            let metadata = graph
                .completed_metadata_map(&global, owner, &info, 0)
                .map_err(|e| e.to_string())?;
            let root = graph
                .recipe(Recipe::Vector(vec![inline, metadata]))
                .map_err(|e| e.to_string())?;
            match route {
                Route::SingleWalk => graph
                    .materialize_using(root, &bridge)
                    .map_err(|e| e.to_string()),
                Route::WholeController => {
                    let mut cache = RootCache::default();
                    graph.materialize_reader_controller(root, &mut cache)
                        .map_err(|e|e.to_string())?
                        .ok_or_else(|| "whole-build decline cannot substitute for the required identity-preserving route".to_string())
                }
            }
        };
        // A failure is not converted to a successful empty callback trace.
        let value = result?;
        session.collect().map_err(|e| e.to_string())?;
        let projection = session
            .eval("(fn [v] [(count v) (count (nth v 0)) (count (nth v 1))])")
            .map_err(|e| e.to_string())?;
        let shape = session
            .invoke(&projection, &[&value])
            .map_err(|e| e.to_string())?;
        let shape =
            crate::portable_repl::display(&mut session, &shape).map_err(|e| e.to_string())?;
        let trace = session
            .eval("source-order-events")
            .map_err(|e| e.to_string())?;
        let decoded = bridge
            .read(&mut session, &trace, 0..1)
            .map_err(|e| e.to_string())?;
        let Kind::Vector(events) = &decoded.kind else {
            return Err("trace is not a vector".into());
        };
        // Reject a zero-event fixture and ensure BOTH real constructors ran.
        let mut inline_seen = false;
        let mut metadata_seen = false;
        for event in events {
            let Kind::Vector(row) = &event.kind else {
                return Err("trace row is not a vector".into());
            };
            if row.len() != 3 {
                return Err("trace row schema changed".into());
            }
            match &row[0].kind {
                Kind::String(name)
                    if *name == "order.inline".encode_utf16().collect::<Vec<_>>() =>
                {
                    if matches!(route, Route::SingleWalk) && metadata_seen {
                        return Err("baseline single walk did not complete inline map before captured metadata".into());
                    }
                    inline_seen = true;
                }
                Kind::String(name) if *name == "order.meta".encode_utf16().collect::<Vec<_>>() => {
                    metadata_seen = true
                }
                _ => return Err("foreign callback admitted to the filtered trace".into()),
            }
        }
        if !inline_seen || !metadata_seen {
            return Err("fixture did not observe both >8-entry map constructors".into());
        }
        let trace =
            crate::portable_repl::display(&mut session, &trace).map_err(|e| e.to_string())?;
        Ok(Observation { trace, shape })
    })();
    // Restore even when controller selection/health fails; no caught failure is
    // interpreted as conformance, no guest construction retry takes place.
    let restored = original.restore(&mut session).map_err(|e| e.to_string());
    restored?;
    result
}
fn preserves_first_use(delegate: Delegate) {
    for phase in [Phase::Runtime, Phase::Macro] {
        let baseline = observed(phase, Route::SingleWalk, delegate)
            .unwrap_or_else(|e| panic!("existing single walk / {phase:?} / {delegate:?}: {e}"));
        assert_eq!(baseline.shape, "[2 10 10]");
        // Do NOT silently accept native decline or health-only-key rejection.
        let controlled = observed(phase, Route::WholeController, delegate)
            .unwrap_or_else(|e| panic!("whole controller / {phase:?} / {delegate:?}: {e}"));
        assert_eq!(controlled.shape, baseline.shape);
        assert_eq!(
            controlled.trace, baseline.trace,
            "first-use callback order and entry memos must survive source boundaries; {phase:?}/{delegate:?}"
        );
    }
}
#[test]
fn real_completed_metadata_after_inline_map_keeps_original_delegate_callback_order() {
    preserves_first_use(Delegate::OriginalEveryTime);
}
#[test]
fn real_completed_metadata_after_inline_map_keeps_nil_aware_callback_order() {
    preserves_first_use(Delegate::NilAware);
}
fn local_sources(node: &Hir, out: &mut Vec<Arc<hir::SourceAnalysis>>) {
    if let Some(source) = &node.source {
        if matches!(source.resolved, Some(hir::SourceBinding::Local(_))) {
            out.push(source.clone());
            return;
        }
        match source.node.as_deref() {
            Some(hir::SourceNode::Bindings { body, .. }) => {
                local_sources(body, out);
                return;
            }
            Some(hir::SourceNode::Do { statements, result }) => {
                for child in statements.iter() {
                    local_sources(child, out);
                }
                local_sources(result, out);
                return;
            }
            _ => {}
        }
    }
    if let Expression::Do(items) = &node.kind {
        for child in items {
            local_sources(child, out);
        }
    }
}
#[test]
fn actual_environment_producer_reuses_same_locals_projection_and_admits_both_records() {
    let text = "(let* [watched 17] (do watched watched))";
    let forms = suss_reader::forms::read_forms(text).unwrap();
    let tree = hir::analyze(&forms, 0..text.len()).unwrap();
    let mut references = Vec::new();
    local_sources(&tree, &mut references);
    assert_eq!(
        references.len(),
        2,
        "two genuine local reference ASTs, not cloned owner fixtures"
    );
    assert!(
        !Arc::ptr_eq(&references[0], &references[1]),
        "different source occurrences"
    );
    assert!(Arc::ptr_eq(&references[0].locals, &references[1].locals));
    assert!(Arc::ptr_eq(&references[0].fields, &references[1].fields));
    assert!(
        !references[0].locals.is_empty(),
        "exercise actual local child recipes too"
    );
    let mut session = Session::new_macro().unwrap();
    let bridge = FormBridge::new(&mut session).unwrap();
    let mut graph = AnalysisGraph::new(&bridge, &mut session);
    graph.capture_roots = true;
    graph.direct_reader_roots = true;
    let mut environments = Vec::new();
    for source in &references {
        environments.push(
            graph
                .environment_record(
                    &source.namespace_snapshot,
                    &source.scope,
                    &source.locals,
                    &source.fields,
                    &source.function_scopes,
                    source.context,
                    0,
                )
                .unwrap(),
        );
    }
    graph.finish_recipes().unwrap();
    let pair = (&references[0].locals, &references[0].fields);
    let matching: Vec<_> = graph
        .source_roots
        .roots
        .iter()
        .filter(|capture| {
            matches!(&capture.owner, Owner::Locals(locals, fields)
            if Arc::ptr_eq(locals, pair.0) && Arc::ptr_eq(fields, pair.1))
        })
        .collect();
    assert_eq!(
        matching.len(),
        1,
        "actual producer must reuse one native locals projection/capture for the same pair"
    );
    let alias = matching[0].alias;
    let native = matching[0].native;
    assert!(matches!(graph.recipes[native], Some(Recipe::Map(_))));
    // Inspect the genuine produced environment entries after full task expansion,
    // rather than passing a manually shared Map ID to capture_source_root.
    for environment in &environments {
        let Recipe::Map(entries) = graph.recipes[*environment].as_ref().unwrap() else {
            panic!("actual environment_record returns its native field map");
        };
        let entries = entries.clone();
        assert_eq!(
            graph.declaration_field_value(&entries, "locals").unwrap(),
            Some(alias)
        );
    }
    let outer = graph.recipe(Recipe::Vector(environments)).unwrap();
    let outer = graph.reader_request_export(outer).unwrap();
    assert!(
        graph.admit_reader_controller(outer).unwrap().is_some(),
        "two actual environment producers cannot receive identity-losing blanket decline"
    );
}
