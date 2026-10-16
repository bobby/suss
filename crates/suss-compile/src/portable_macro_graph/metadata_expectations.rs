//! Final declaration-metadata expectations derived solely from native recipes.
//! This supplies logical facts, not operational-storage or memo authority.
use super::{Recipe, Result, Value, source_facts::LogicalSourceWitness};
use crate::{
    portable::{
        hir::Literal,
        resolve::{DefinitionInfo, Global},
    },
    portable_macro_data::{FormBridge, MetadataShape},
    portable_session::{Session, SessionError, SessionValue},
};
use std::{rc::Rc, sync::Arc};
use suss_reader::forms::{Form, Kind};

pub(super) struct CapturedMetadata {
    pub(super) global: Global,
    pub(super) revision: Arc<DefinitionInfo>,
    pub(super) recipe: Value,
    expectation: Option<Option<Rc<MetadataExpectation>>>,
}
pub(super) struct MetadataExpectation {
    global: Global,
    revision: Arc<DefinitionInfo>,
    witness: LogicalSourceWitness,
}
fn invalid(message: &str) -> SessionError {
    SessionError::Host(wasmtime::Error::msg(message.to_owned()))
}
fn reserve<T>(session: &mut Session, values: &mut Vec<T>, extra: usize) -> Result<()> {
    session.charge_host_work(0)?;
    if extra > values.capacity() - values.len() {
        let needed = values
            .len()
            .checked_add(extra)
            .ok_or_else(|| invalid("Metadata expectation allocation overflow"))?;
        let count = needed
            .max(
                values
                    .capacity()
                    .checked_mul(2)
                    .ok_or_else(|| invalid("Metadata expectation allocation overflow"))?,
            )
            .max(4);
        let bytes = count
            .checked_mul(std::mem::size_of::<T>())
            .ok_or_else(|| invalid("Metadata expectation allocation overflow"))?;
        session.charge_host_work(bytes as u64)?;
        session.charge_host_work((values.len() * std::mem::size_of::<T>()) as u64)?;
        values
            .try_reserve_exact(count - values.len())
            .map_err(|_| invalid("Metadata expectation allocation failed"))?;
    }
    Ok(())
}
fn push<T>(session: &mut Session, values: &mut Vec<T>, value: T) -> Result<()> {
    session.charge_host_work(1)?;
    reserve(session, values, 1)?;
    values.push(value);
    Ok(())
}
fn text(session: &mut Session, value: &str) -> Result<String> {
    session.charge_host_work(value.len() as u64)?;
    let mut result = String::with_capacity(value.len());
    for character in value.chars() {
        session.charge_host_work(character.len_utf8() as u64 + 1)?;
        result.push(character);
    }
    Ok(result)
}
fn units(session: &mut Session, value: &[u16]) -> Result<Vec<u16>> {
    let mut result = Vec::new();
    reserve(session, &mut result, value.len())?;
    for chunk in value.chunks(256) {
        session.charge_host_work(chunk.len() as u64)?;
        result.extend_from_slice(chunk);
    }
    Ok(result)
}
fn plain(kind: Kind) -> Form {
    Form {
        span: 0..0,
        metadata: vec![],
        kind,
    }
}
fn dependencies(session: &mut Session, recipe: &Recipe) -> Result<Vec<Value>> {
    let count = match recipe {
        Recipe::Alias(_) => 1,
        Recipe::List(v)
        | Recipe::Vector(v)
        | Recipe::ReaderVector(v)
        | Recipe::Set(v)
        | Recipe::ReaderSet(v) => v.len(),
        Recipe::Map(v) | Recipe::ReaderMap(v) => v
            .len()
            .checked_mul(2)
            .ok_or_else(|| invalid("Metadata expectation edge count overflow"))?,
        Recipe::Metadata { set_items, .. } => set_items
            .len()
            .checked_add(2)
            .ok_or_else(|| invalid("Metadata expectation edge count overflow"))?,
        _ => 0,
    };
    let mut result = Vec::new();
    reserve(session, &mut result, count)?;
    match recipe {
        Recipe::Alias(value) => push(session, &mut result, *value)?,
        Recipe::List(v)
        | Recipe::Vector(v)
        | Recipe::ReaderVector(v)
        | Recipe::Set(v)
        | Recipe::ReaderSet(v) => {
            for value in v {
                push(session, &mut result, *value)?;
            }
        }
        Recipe::Map(v) | Recipe::ReaderMap(v) => {
            for (key, value) in v {
                push(session, &mut result, *key)?;
                push(session, &mut result, *value)?;
            }
        }
        Recipe::Metadata {
            value,
            metadata,
            set_items,
            ..
        } => {
            push(session, &mut result, *value)?;
            for item in set_items {
                push(session, &mut result, *item)?;
            }
            push(session, &mut result, *metadata)?;
        }
        _ => {}
    }
    Ok(result)
}
struct Frame {
    id: Value,
    dependencies: Vec<Value>,
    next: usize,
    inputs: Vec<Option<Form>>,
}
fn frame(
    session: &mut Session,
    recipes: &[Option<Recipe>],
    id: Value,
    active: &[Frame],
) -> Result<Frame> {
    session.charge_host_work(1)?;
    for ancestor in active {
        session.charge_host_work(1)?;
        if ancestor.id == id {
            return Err(invalid("Cyclic metadata expectation recipe"));
        }
    }
    let recipe = recipes
        .get(id)
        .and_then(Option::as_ref)
        .ok_or_else(|| invalid("Missing or unfinished metadata expectation recipe"))?;
    Ok(Frame {
        id,
        dependencies: dependencies(session, recipe)?,
        next: 0,
        inputs: Vec::new(),
    })
}
fn discard(session: &mut Session, value: Form) -> Result<()> {
    super::source_facts::discard_form(session, value)
}
fn agrees(shape: MetadataShape, kind: &Kind) -> bool {
    match (shape, kind) {
        (MetadataShape::Symbol, Kind::Symbol(_))
        | (MetadataShape::Vector, Kind::Vector(_))
        | (MetadataShape::Map { .. }, Kind::Map(_)) => true,
        (MetadataShape::List { empty }, Kind::List(values)) => empty == values.is_empty(),
        (MetadataShape::Set, _) => true, // Its logical body comes from set_items.
        _ => false,
    }
}
fn finish(
    session: &mut Session,
    recipe: &Recipe,
    inputs: Vec<Option<Form>>,
) -> Result<Option<Form>> {
    session.charge_host_work(inputs.len() as u64 + 1)?;
    if let Recipe::Metadata {
        shape, set_items, ..
    } = recipe
    {
        session.charge_host_work(4)?;
        if !matches!(shape, MetadataShape::Set) && !set_items.is_empty() {
            return Err(invalid("Non-set metadata recipe has set operands"));
        }
        if inputs
            .first()
            .and_then(Option::as_ref)
            .is_some_and(|base| !agrees(*shape, &base.kind))
        {
            return Err(invalid("Metadata recipe disagrees with its source shape"));
        }
        if inputs
            .last()
            .and_then(Option::as_ref)
            .is_some_and(|meta| !matches!(meta.kind, Kind::Map(_) | Kind::Nil))
        {
            return Err(invalid("Expected source metadata must be a map or nil"));
        }
    }
    if inputs.iter().any(Option::is_none) {
        for value in inputs.into_iter().flatten() {
            discard(session, value)?;
        }
        return Ok(None);
    }
    let mut values = Vec::new();
    reserve(session, &mut values, inputs.len())?;
    for input in inputs {
        values.push(input.unwrap());
    }
    let kind = match recipe {
        Recipe::Scalar(Literal::Undefined) => return Ok(None),
        Recipe::Scalar(Literal::Nil) => Kind::Nil,
        Recipe::Scalar(Literal::Bool(v)) => Kind::Bool(*v),
        Recipe::Scalar(Literal::Number(v)) => Kind::Number(*v),
        Recipe::Scalar(Literal::String(v)) => Kind::String(units(session, v)?),
        Recipe::Identifier {
            namespace,
            name,
            keyword,
        } => {
            let namespace = namespace
                .as_ref()
                .map(|value| text(session, value))
                .transpose()?;
            let name = text(session, name)?;
            if *keyword {
                Kind::Keyword(suss_reader::Keyword { namespace, name })
            } else {
                Kind::Symbol(suss_reader::Symbol { namespace, name })
            }
        }
        Recipe::Form(form) => {
            return Ok(Some(super::source_facts::normalized_copy(session, form)?));
        }
        Recipe::Alias(_) => return Ok(values.pop()),
        Recipe::List(_) => Kind::List(values),
        Recipe::Vector(_) | Recipe::ReaderVector(_) => Kind::Vector(values),
        Recipe::Map(_) | Recipe::ReaderMap(_) => Kind::Map(values),
        Recipe::Set(_) | Recipe::ReaderSet(_) => Kind::Set(values),
        Recipe::Metadata {
            shape, set_items, ..
        } => {
            let metadata = values
                .pop()
                .ok_or_else(|| invalid("Missing expected metadata value"))?;
            let mut inputs = values.into_iter();
            let mut base = inputs
                .next()
                .ok_or_else(|| invalid("Missing expected metadata receiver"))?;
            match shape {
                MetadataShape::Set => {
                    let mut items = Vec::new();
                    reserve(session, &mut items, set_items.len())?;
                    for item in inputs {
                        push(session, &mut items, item)?;
                    }
                    discard(session, base)?;
                    base = plain(Kind::Set(items));
                }
                _ => {}
            }
            for old in std::mem::take(&mut base.metadata) {
                discard(session, old)?;
            }
            match metadata.kind {
                Kind::Nil => {}
                Kind::Map(_) => push(session, &mut base.metadata, metadata)?,
                _ => return Err(invalid("Expected source metadata must be a map or nil")),
            }
            return Ok(Some(base));
        }
    };
    Ok(Some(plain(kind)))
}
fn project(session: &mut Session, recipes: &[Option<Recipe>], root: Value) -> Result<Option<Form>> {
    let mut frames = Vec::new();
    let first = frame(session, recipes, root, &frames)?;
    push(session, &mut frames, first)?;
    while let Some(current) = frames.last_mut() {
        session.charge_host_work(1)?;
        if current.next < current.dependencies.len() {
            let id = current.dependencies[current.next];
            current.next += 1;
            let next = frame(session, recipes, id, &frames)?;
            push(session, &mut frames, next)?;
            continue;
        }
        let done = frames.pop().unwrap();
        let value = finish(session, recipes[done.id].as_ref().unwrap(), done.inputs)?;
        if let Some(parent) = frames.last_mut() {
            push(session, &mut parent.inputs, value)?;
        } else {
            return Ok(value);
        }
    }
    unreachable!("a source expectation has a root")
}
pub(super) fn capture(
    session: &mut Session,
    sources: &mut Vec<CapturedMetadata>,
    global: &Global,
    revision: &Arc<DefinitionInfo>,
    recipe: Value,
) -> Result<()> {
    session.charge_host_work((global.namespace().len() + global.name().len()) as u64 + 1)?;
    let source = CapturedMetadata {
        global: global.clone(),
        revision: revision.clone(),
        recipe,
        expectation: None,
    };
    push(session, sources, source)
}
impl super::AnalysisGraph<'_> {
    // A caller cannot pair a captured declaration with another recipe array.
    pub(super) fn metadata_expectation(
        &mut self,
        index: usize,
    ) -> Result<Option<Rc<MetadataExpectation>>> {
        let _request = self.session.operation_budget_scope()?;
        self.finish_recipes()?;
        self.session.charge_host_work(3)?;
        let source = self.metadata_sources.get(index)
            .ok_or_else(|| invalid("Unknown captured metadata source"))?;
        if let Some(cached) = &source.expectation {
            return Ok(cached.clone());
        }
        let expected = self.capture_metadata_expectation(index)?;
        // Only successful captures (including a deliberate decline) are cached.
        // Errors leave this slot uncomputed and cannot publish a partial witness.
        self.session.charge_host_work(2)?;
        self.metadata_sources[index].expectation = Some(expected.clone());
        Ok(expected)
    }
    fn capture_metadata_expectation(
        &mut self,
        index: usize,
    ) -> Result<Option<Rc<MetadataExpectation>>> {
        self.session.charge_host_work(1)?;
        let source = self
            .metadata_sources
            .get(index)
            .ok_or_else(|| invalid("Unknown captured metadata source"))?;
        let Some(form) = project(self.session, &self.recipes, source.recipe)? else {
            return Ok(None);
        };
        let Some(witness) = LogicalSourceWitness::from_normalized_source(self.session, form)?
        else {
            return Ok(None);
        };
        self.session.charge_host_work(
            (source.global.namespace().len() + source.global.name().len()) as u64 + 1,
        )?;
        let global = source.global.clone();
        self.session.charge_host_work(0)?;
        self.session.charge_host_work((std::mem::size_of::<MetadataExpectation>()
            + 2 * std::mem::size_of::<usize>()) as u64)?;
        Ok(Some(Rc::new(MetadataExpectation {
            global,
            revision: source.revision.clone(),
            witness,
        })))
    }
}
impl MetadataExpectation {
    pub(super) fn global(&self) -> &Global {
        &self.global
    }
    pub(super) fn revision(&self) -> &Arc<DefinitionInfo> {
        &self.revision
    }
    pub(super) fn matches_logical_source(
        &self,
        session: &mut Session,
        bridge: &FormBridge,
        value: &SessionValue,
    ) -> Result<bool> {
        self.witness.matches_logical_source(session, bridge, value)
    }
}

#[cfg(test)]
mod tests {
    use super::super::AnalysisGraph;
    use super::*;
    use crate::portable::{hir::SourceNamespace, resolve::Phase};
    fn source(text: &str) -> Form {
        let mut forms =
            suss_reader::forms::resolve_conditionals(suss_reader::forms::read_forms(text).unwrap())
                .unwrap();
        assert_eq!(forms.len(), 1);
        forms.pop().unwrap()
    }
    fn session(phase: Phase) -> Session {
        match phase {
            Phase::Runtime => Session::new_repl().unwrap(),
            Phase::Macro => Session::new_macro().unwrap(),
        }
    }
    fn count(session: &mut Session) -> usize {
        let value = session.eval("expectation-events").unwrap();
        crate::portable_repl::display(session, &value)
            .unwrap()
            .parse()
            .unwrap()
    }
    #[test]
    fn expectation_capture_errors_remain_uncached_and_retry_recovers() {
        let mut session = Session::new_macro().unwrap();
        session.eval("(def ^{:doc \"retry\"} expectation-retry 17)").unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        let snapshot = session.compilation_snapshot();
        let namespace = SourceNamespace::capture(&snapshot.environment, Phase::Macro);
        let (global, revision) = namespace.declarations.iter()
            .find(|(g, _)| g.name() == "expectation-retry").unwrap();
        let mut graph = AnalysisGraph::new(&bridge, &mut session);
        graph.declaration(global, Some(revision), 0).unwrap();
        graph.finish_recipes().unwrap();
        graph.session.set_operation_fuel(64);
        let operation = graph.session.operation_budget_scope().unwrap();
        let error = graph.metadata_expectation(0).err().expect("capture must exhaust fuel");
        assert!(matches!(error, SessionError::Trap(ref error)
            if error.downcast_ref::<wasmtime::Trap>() == Some(&wasmtime::Trap::OutOfFuel)), "{error:?}");
        assert!(graph.metadata_sources[0].expectation.is_none());
        drop(operation);
        graph.session.set_operation_fuel(crate::portable::DEFAULT_OPERATION_FUEL);
        let expected = graph.metadata_expectation(0).unwrap().unwrap();
        let operation = graph.session.operation_budget_scope().unwrap();
        graph.session.interrupt_handle().interrupt();
        assert!(graph.metadata_expectation(0).err().unwrap().is_interrupt());
        drop(operation);
        let retry = graph.metadata_expectation(0).unwrap().unwrap();
        assert!(Rc::ptr_eq(&expected, &retry));
    }
    #[test]
    fn metadata_expectations_use_final_overlays_and_own_declaration_revision() {
        for phase in [Phase::Runtime, Phase::Macro] {
            let mut session = session(phase);
            let bridge = FormBridge::new(&mut session).unwrap();
            let extract = session
                .eval("(fn [declaration] (get declaration :meta))")
                .unwrap();
            let mut previous = None;
            for (text, expected) in [
                (
                    "(def ^{:top-fn {:meta {:custom true}}} final-metadata (fn [] 17))",
                    "{:custom true}",
                ),
                (
                    "(def ^{:top-fn {:meta nil}} final-metadata (fn [] 17))",
                    "nil",
                ),
                (
                    "(def ^{:top-fn {:meta {}}} final-metadata (fn [] 17))",
                    "{}",
                ),
            ] {
                session.eval(text).unwrap();
                let snapshot = session.compilation_snapshot();
                let namespace = SourceNamespace::capture(&snapshot.environment, phase);
                let (global, revision) = namespace
                    .declarations
                    .iter()
                    .find(|(global, _)| global.name() == "final-metadata")
                    .map(|(global, revision)| (global.clone(), revision.clone()))
                    .unwrap();
                let (expectation, declaration) = {
                    let _request = session.operation_budget_scope().unwrap();
                    let mut graph = AnalysisGraph::new(&bridge, &mut session);
                    let root = graph.declaration(&global, Some(&revision), 0).unwrap();
                    assert_eq!(graph.metadata_sources.len(), 1);
                    let expectation = graph.metadata_expectation(0).unwrap().unwrap();
                    assert_eq!(expectation.global(), &global);
                    assert!(Arc::ptr_eq(expectation.revision(), &revision));
                    let declaration = graph.materialize(root).unwrap();
                    (expectation, declaration)
                };
                if let Some(previous) = previous.replace(expectation.revision().clone()) {
                    assert!(
                        !Arc::ptr_eq(&previous, expectation.revision()),
                        "new declaration revision must keep a distinct owner"
                    );
                }
                let metadata = session.invoke(&extract, &[&declaration]).unwrap();
                session.collect().unwrap();
                assert!(
                    expectation
                        .matches_logical_source(&mut session, &bridge, &metadata)
                        .unwrap()
                );
                assert_eq!(
                    crate::portable_repl::display(&mut session, &metadata).unwrap(),
                    expected
                );
                if expected == "{:custom true}" {
                    let mutate = session
                        .eval("(fn [metadata] (aset (.-arr metadata) 1 false))")
                        .unwrap();
                    session.invoke(&mutate, &[&metadata]).unwrap();
                    assert!(
                        !expectation
                            .matches_logical_source(&mut session, &bridge, &metadata)
                            .unwrap(),
                        "captured source facts must not refresh from guest mutation"
                    );
                }
            }
        }
    }
    #[test]
    fn metadata_expectations_project_metadata_sets_without_callbacks() {
        for phase in [Phase::Runtime, Phase::Macro] {
            let mut session = session(phase);
            session
                .eval("(def expectation-events 0) (def expectation-original-hash suss.core/hash)")
                .unwrap();
            session.enter_namespace("suss.core").unwrap();
            session.eval("(def hash (fn [key] (do (if (number? key) (set! user/expectation-events (+ user/expectation-events 1)) nil) (user/expectation-original-hash key))))").unwrap();
            session.enter_namespace("user").unwrap();
            let bridge = FormBridge::new(&mut session).unwrap();
            let (witness, value) = {
                let _request = session.operation_budget_scope().unwrap();
                let mut graph = AnalysisGraph::new(&bridge, &mut session);
                let root = graph
                    .form(
                        &source("^{:m 17} #{101 102 103 104 105 106 107 108 109}"),
                        0,
                    )
                    .unwrap();
                assert!(matches!(
                    graph.recipes[root],
                    Some(Recipe::Metadata {
                        shape: MetadataShape::Set,
                        ..
                    })
                ));
                assert_eq!(count(graph.session), 0);
                let form = project(graph.session, &graph.recipes, root)
                    .unwrap()
                    .unwrap();
                assert!(matches!(&form.kind, Kind::Set(items) if items.len() == 9));
                let witness = LogicalSourceWitness::from_normalized_source(graph.session, form)
                    .unwrap()
                    .unwrap();
                assert_eq!(
                    count(graph.session),
                    0,
                    "native projection and preparation must execute no construction callback"
                );
                let value = graph.materialize(root).unwrap();
                (witness, value)
            };
            assert_eq!(
                count(&mut session),
                9,
                "materialization retains the existing numeric hash effects"
            );
            session.collect().unwrap();
            assert!(
                witness
                    .matches_logical_source(&mut session, &bridge, &value)
                    .unwrap()
            );
            assert_eq!(
                count(&mut session),
                9,
                "logical validation must not rehash guest data"
            );
        }
    }
    #[test]
    fn metadata_expectations_replace_metadata_and_preserve_metadata_on_metadata() {
        for phase in [Phase::Runtime, Phase::Macro] {
            let mut session = session(phase);
            let bridge = FormBridge::new(&mut session).unwrap();
            let (witness, value) = {
                let _request = session.operation_budget_scope().unwrap();
                let mut graph = AnalysisGraph::new(&bridge, &mut session);
                let base = graph.recipe(Recipe::Form(source("^:old [1 2]"))).unwrap();
                let metadata = graph
                    .recipe(Recipe::Form(source("^:on-meta {:m 17}")))
                    .unwrap();
                let root = graph
                    .recipe(Recipe::Metadata {
                        shape: MetadataShape::Vector,
                        value: base,
                        metadata,
                        set_items: vec![],
                    })
                    .unwrap();
                let expected = project(graph.session, &graph.recipes, root)
                    .unwrap()
                    .unwrap();
                assert_eq!(expected.metadata.len(), 1);
                assert_eq!(expected.metadata[0].metadata.len(), 1);
                let witness = LogicalSourceWitness::from_normalized_source(graph.session, expected)
                    .unwrap()
                    .unwrap();
                let value = graph.materialize(root).unwrap();
                (witness, value)
            };
            session.collect().unwrap();
            assert!(
                witness
                    .matches_logical_source(&mut session, &bridge, &value)
                    .unwrap()
            );
            let actual = bridge.read(&mut session, &value, 0..0).unwrap();
            let Kind::Map(pairs) = &actual.metadata[0].kind else {
                panic!("metadata map")
            };
            assert!(matches!(&pairs[0].kind, Kind::Keyword(key) if key.name == "m"));
            assert_eq!(pairs.len(), 2, "replacement must remove :old");
            let Kind::Map(pairs) = &actual.metadata[0].metadata[0].kind else {
                panic!("metadata on metadata")
            };
            assert!(matches!(&pairs[0].kind, Kind::Keyword(key) if key.name == "on-meta"));
            assert!(matches!(pairs[1].kind, Kind::Bool(true)));
            let mutate = session
                .eval("(fn [value] (aset (.-arr (meta (meta value))) 1 false))")
                .unwrap();
            session.invoke(&mutate, &[&value]).unwrap();
            session.collect().unwrap();
            assert!(
                !witness
                    .matches_logical_source(&mut session, &bridge, &value)
                    .unwrap(),
                "changing only metadata-on-metadata must reject the independent witness"
            );
        }
    }
    #[test]
    fn metadata_expectations_check_declined_siblings_and_bound_expanded_dags() {
        let mut session = Session::new().unwrap();
        let odd = plain(Kind::Map(vec![source(":a")]));
        let error = LogicalSourceWitness::from_normalized_source(&mut session, odd)
            .err()
            .unwrap();
        assert!(error.to_string().contains("paired entries"));
        let hidden_odd = plain(Kind::Vector(vec![
            plain(Kind::Map(vec![source(":a")])),
            plain(Kind::Map(vec![source("[1]"), source("2")])),
        ]));
        let error = LogicalSourceWitness::from_normalized_source(&mut session, hidden_odd)
            .err()
            .unwrap();
        assert!(
            error.to_string().contains("paired entries"),
            "admission decline must not hide malformed siblings"
        );
        let missing = vec![
            Some(Recipe::Scalar(Literal::Undefined)),
            Some(Recipe::Vector(vec![0, 2])),
            None,
        ];
        assert!(
            session
                .with_operation_budget(|session| project(session, &missing, 1))
                .unwrap_err()
                .to_string()
                .contains("Missing or unfinished")
        );
        let cyclic = vec![
            Some(Recipe::Scalar(Literal::Undefined)),
            Some(Recipe::Vector(vec![0, 2])),
            Some(Recipe::Alias(1)),
        ];
        assert!(
            session
                .with_operation_budget(|session| project(session, &cyclic, 1))
                .unwrap_err()
                .to_string()
                .contains("Cyclic")
        );
        let malformed = vec![
            Some(Recipe::Scalar(Literal::Undefined)),
            Some(Recipe::Scalar(Literal::Nil)),
            Some(Recipe::Metadata {
                shape: MetadataShape::Vector,
                value: 1,
                metadata: 1,
                set_items: vec![0],
            }),
        ];
        assert!(
            session
                .with_operation_budget(|session| project(session, &malformed, 2))
                .unwrap_err()
                .to_string()
                .contains("Non-set metadata")
        );
        let bad_meta = vec![
            Some(Recipe::Scalar(Literal::Undefined)),
            Some(Recipe::Scalar(Literal::Bool(true))),
            Some(Recipe::Metadata {
                shape: MetadataShape::Vector,
                value: 0,
                metadata: 1,
                set_items: vec![],
            }),
        ];
        assert!(
            session
                .with_operation_budget(|session| project(session, &bad_meta, 2))
                .unwrap_err()
                .to_string()
                .contains("map or nil")
        );
        let declined = vec![
            Some(Recipe::Scalar(Literal::Undefined)),
            Some(Recipe::Vector(vec![0])),
        ];
        assert!(
            session
                .with_operation_budget(|session| project(session, &declined, 1))
                .unwrap()
                .is_none()
        );
        let small = vec![
            Some(Recipe::Scalar(Literal::Number(-0.0))),
            Some(Recipe::Vector(vec![0, 0])),
        ];
        let cost = session.with_operation_budget(|session| {
            let before = session.operation_fuel_remaining()?;
            let form = project(session, &small, 1)?.unwrap();
            assert!(matches!(&form.kind, Kind::Vector(items) if items.len()==2 && items.iter().all(|v| matches!(v.kind, Kind::Number(n) if n.to_bits() == (-0.0f64).to_bits()))));
            Ok(before - session.operation_fuel_remaining()?)
        }).unwrap();
        let allowance = cost * 2 - 1;
        assert!(allowance < crate::portable::DEFAULT_OPERATION_FUEL);
        session.set_operation_fuel(allowance);
        let error = session
            .with_operation_budget(|session| {
                assert!(project(session, &small, 1)?.is_some());
                project(session, &small, 1)
            })
            .unwrap_err();
        assert!(
            matches!(error, SessionError::Trap(ref error) if error.downcast_ref::<wasmtime::Trap>() == Some(&wasmtime::Trap::OutOfFuel)),
            "{error:?}"
        );
        session
            .with_operation_budget(|session| {
                assert!(project(session, &small, 1)?.is_some());
                Ok(())
            })
            .unwrap();
        let mut diamond = vec![Some(Recipe::Scalar(Literal::Nil))];
        for index in 0..20 {
            diamond.push(Some(Recipe::Vector(vec![index, index])));
        }
        let error = session
            .with_operation_budget(|session| project(session, &diamond, 20))
            .unwrap_err();
        assert!(
            matches!(error, SessionError::Trap(ref error) if error.downcast_ref::<wasmtime::Trap>() == Some(&wasmtime::Trap::OutOfFuel)),
            "{error:?}"
        );
        let interrupt = session.host_work_interrupt();
        let error = session
            .with_operation_budget(|session| {
                interrupt.store(true, std::sync::atomic::Ordering::SeqCst);
                project(session, &small, 1)
            })
            .unwrap_err();
        assert!(
            matches!(error, SessionError::Trap(ref error) if error.downcast_ref::<wasmtime::Trap>() == Some(&wasmtime::Trap::Interrupt)),
            "{error:?}"
        );
        session
            .with_operation_budget(|session| {
                assert!(project(session, &small, 1)?.is_some());
                Ok(())
            })
            .unwrap();
    }
}
