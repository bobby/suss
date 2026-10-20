//! Identity-only source transport. These private observations expose no
//! Certificate, CheckedChild, WriteDomain, receipt or nil-write capability.
//! All evidence is the ORIGINAL native factory record plus exact input edges;
//! an exposed guest object is never recaptured to manufacture source authority.
use super::first_use_walk::{Context, Entry, Observed, Observer};
use super::*;
use crate::portable::metered_map::{Context as MapContext, TryOrderedMap};
use crate::portable_macro_data::FormBridge;
use crate::portable_session::construction_records::ConstructionRecords;
use std::cmp::Ordering;

#[path = "first_use_data/original_inputs.rs"]
mod original_inputs;
pub(crate) use original_inputs::OriginalDataInputs;

type SessionValue = crate::portable_session::SessionValue;
#[derive(Clone)]
enum Evidence {
    Native(Rc<Native>),
    Source(Rc<Root>),
}
struct Native {
    value: SessionValue,
    records: ConstructionRecords,
    inputs: Vec<Evidence>,
    release: Option<Box<ReleaseFrame>>,
}
pub(super) struct Root {
    expected: Rc<Expectation>,
    actual: Option<Evidence>,
    // Exact retained alias of actual.value(), not a validation result. This
    // makes value access constant-stack even through many Source wrappers.
    value: SessionValue,
    release: Option<Box<ReleaseFrame>>,
}
impl Evidence {
    fn value(&self) -> &SessionValue {
        match self {
            Self::Native(node) => &node.value,
            Self::Source(root) => &root.value,
        }
    }
    fn identity(&self) -> (u8, usize) {
        match self {
            Self::Native(node) => (0, Rc::as_ptr(node) as usize),
            Self::Source(root) => (1, Rc::as_ptr(root) as usize),
        }
    }
    fn retained(&self, session: &mut Session) -> Result<Self> {
        session.charge_host_work(2)?;
        Ok(self.clone())
    }
}

// A private, prepaid release frame belongs to EVERY evidence allocation before
// it can be published. Frames form an intrusive work list during final release;
// no release-time Vec growth, Box allocation, Session access or fuel is needed.
// This is storage for teardown, never a source/domain/validation capability.
struct ReleaseFrame {
    edges: Vec<Evidence>,
    next: Option<Box<ReleaseFrame>>,
}
impl ReleaseFrame {
    fn allocate(session: &mut Session, source_slot: bool, edges: usize) -> Result<Box<Self>> {
        // Box backing/init+retirement, bounded list/unwrap/drop controls, plus
        // per-edge move, Rc unique-test, shared decrement and queue controls.
        let work = std::mem::size_of::<Self>()
            .checked_mul(2)
            .and_then(|n| n.checked_add(2 * std::mem::size_of::<usize>() + 32))
            .and_then(|n| edges.checked_mul(32).and_then(|e| n.checked_add(e)))
            .ok_or_else(|| invalid("Evidence release prepayment overflow"))?;
        session.charge_host_work(work as u64)?;
        let mut pending = Vec::new();
        if source_slot {
            // One Source edge is moved into this RESERVED slot on last drop.
            // Native frames instead take ownership of the existing input Vec.
            reserve(session, &mut pending, 1)?;
        }
        Ok(Box::new(Self {
            edges: pending,
            next: None,
        }))
    }
}
impl Drop for ReleaseFrame {
    fn drop(&mut self) {
        // The drain loop unlinks and empties each processed frame. Return here
        // rather than re-entering the drain function for those empty frames.
        if self.edges.is_empty() && self.next.is_none() {
            return;
        }
        // Also covers an owned frame unwinding before normal publication. The
        // current frame is emptied before its automatic field destructors run.
        release_edges(std::mem::take(&mut self.edges), self.next.take());
    }
}
fn release_edges(mut edges: Vec<Evidence>, mut work: Option<Box<ReleaseFrame>>) {
    loop {
        while let Some(edge) = edges.pop() {
            let next = match edge {
                Evidence::Native(node) => match Rc::try_unwrap(node) {
                    Ok(mut node) => {
                        let mut frame = node.release.take().expect("live native release frame");
                        std::mem::swap(&mut frame.edges, &mut node.inputs);
                        // node now has no recursive owning evidence. Its Drop
                        // and original factory-record/SessionValue fields are
                        // bounded wrt the evidence graph.
                        Some(frame)
                    }
                    // try_unwrap failure proves another strong owner remains.
                    // Dropping this one Rc cannot invoke the node destructor.
                    Err(shared) => {
                        drop(shared);
                        None
                    }
                },
                Evidence::Source(root) => match Rc::try_unwrap(root) {
                    Ok(mut root) => {
                        let mut frame = root.release.take().expect("live source release frame");
                        if let Some(actual) = root.actual.take() {
                            frame.edges.push(actual); // preallocated ONE slot
                        }
                        Some(frame)
                    }
                    Err(shared) => {
                        drop(shared);
                        None
                    }
                },
            };
            if let Some(mut next) = next {
                next.next = work.take();
                work = Some(next);
            }
        }
        let Some(mut next) = work.take() else {
            return;
        };
        work = next.next.take();
        edges = std::mem::take(&mut next.edges);
        // Dropping the EMPTY frame returns at its guard without re-entering
        // release_edges. Stack depth is independent of the original DAG.
    }
}
impl Native {
    fn allocate(
        session: &mut Session,
        value: &SessionValue,
        records: ConstructionRecords,
        inputs: Vec<Evidence>,
    ) -> Result<Rc<Self>> {
        let release = ReleaseFrame::allocate(session, false, inputs.len())?;
        // Native backing, publication move, try_unwrap move and retirement.
        // Release processing moves the payload out of its Rc allocation once.
        session.charge_host_work(
            (4 * std::mem::size_of::<Self>() + 2 * std::mem::size_of::<usize>() + 8) as u64,
        )?;
        Ok(Rc::new(Self {
            value: value.clone(),
            records,
            inputs,
            release: Some(release),
        }))
    }
}
impl Drop for Native {
    fn drop(&mut self) {
        if let Some(mut release) = self.release.take() {
            std::mem::swap(&mut release.edges, &mut self.inputs);
            // Dropping this frame drains all newly unique descendants with the
            // intrusive iterative work list, including failure and eviction.
            drop(release);
        }
    }
}
impl Root {
    fn actual(&self) -> &Evidence {
        self.actual
            .as_ref()
            .expect("only teardown detaches live source input")
    }
    fn allocate(
        session: &mut Session,
        expected: &Rc<Expectation>,
        actual: Evidence,
    ) -> Result<Rc<Self>> {
        let release = ReleaseFrame::allocate(session, true, 1)?;
        // Source backing, publication move, try_unwrap move and retirement.
        session.charge_host_work(
            (4 * std::mem::size_of::<Self>() + 2 * std::mem::size_of::<usize>() + 12) as u64,
        )?;
        // SessionValue/Expectation retain+eventual release included above;
        // this is an exact live value alias, not a guest re-read or proof.
        let value = actual.value().clone();
        Ok(Rc::new(Self {
            expected: expected.clone(),
            actual: Some(actual),
            value,
            release: Some(release),
        }))
    }
}
impl Drop for Root {
    fn drop(&mut self) {
        if let Some(mut release) = self.release.take() {
            if let Some(actual) = self.actual.take() {
                release.edges.push(actual); // preallocated, no reserve/growth
            }
            drop(release);
        }
    }
}

struct Meter<'a>(&'a mut Session);
impl MapContext<(u8, usize)> for Meter<'_> {
    type Error = SessionError;
    fn charge(&mut self, units: u64) -> Result<()> {
        self.0.charge_host_work(units)
    }
    fn compare(&mut self, a: &(u8, usize), b: &(u8, usize)) -> Result<Ordering> {
        self.charge(2)?;
        Ok(a.cmp(b))
    }
}
/// One CLOSED read-only pass only. Never keep this across a constructor or
/// callback. Native Rc identities are an ephemeral work index, not authority.
struct Audit {
    checked: TryOrderedMap<(u8, usize), bool>,
    native: crate::portable_session::certification_audit::CertificationAudit,
}
impl Audit {
    fn new(session: &mut Session) -> Result<Self> {
        session.charge_host_work(2)?;
        Ok(Self {
            checked: TryOrderedMap::default(),
            native: crate::portable_session::certification_audit::CertificationAudit::new(session)?,
        })
    }
    fn check(&mut self, graph: &mut AnalysisGraph<'_>, evidence: &Evidence) -> Result<bool> {
        let mut pending = Vec::new();
        populate(
            graph.session,
            &mut pending,
            std::iter::once((evidence, false)),
        )?;
        while let Some((current, exit)) = pending.pop() {
            graph.session.charge_host_work(2)?;
            let key = current.identity();
            if self.checked.get(&key, &mut Meter(graph.session))?.is_some() {
                continue;
            }
            if !exit {
                populate(
                    graph.session,
                    &mut pending,
                    std::iter::once((current, true)),
                )?;
                match current {
                    Evidence::Native(node) => populate(
                        graph.session,
                        &mut pending,
                        node.inputs.iter().rev().map(|child| (child, false)),
                    )?,
                    Evidence::Source(root) => populate(
                        graph.session,
                        &mut pending,
                        std::iter::once((root.actual(), false)),
                    )?,
                }
                continue;
            }
            let mut valid = true;
            match current {
                Evidence::Native(node) => {
                    graph.session.check_value_owner(&node.value)?;
                    let mut inputs = Vec::new();
                    reserve(graph.session, &mut inputs, node.inputs.len())?;
                    for child in &node.inputs {
                        graph.session.charge_host_work(2)?;
                        valid &= *self
                            .checked
                            .get(&child.identity(), &mut Meter(graph.session))?
                            .ok_or_else(|| invalid("Missing first-use input check"))?;
                        inputs.push(child.value());
                    }
                    #[cfg(test)]
                    let diagnostic_children = valid;
                    let diagnostic_direct = node.records.fresh_direct_status(graph.session)?.0;
                    valid &= diagnostic_direct;
                    let diagnostic_inputs = {
                        // This borrow cannot outlive this closed Audit check.
                        // Its private constructor accepts only the exact current
                        // node's original inputs and this pass's checked map.
                        let mut descendants = OriginalDataInputs::new(
                            graph.session, &node.inputs, &mut self.checked, &node.records,
                        )?;
                        node.records.matches_current_native_inputs_with_descendants(
                            graph.session, &node.value, &inputs, &mut descendants,
                        )?
                    };
                    valid &= diagnostic_inputs;
                    let diagnostic_classes = node
                        .records
                        .matches_class_schemas_audited(graph.session, &mut self.native)?;
                    valid &= diagnostic_classes;
                    let diagnostic_prototypes = node
                        .records
                        .matches_prototype_origins_audited(graph.session, &mut self.native)?;
                    valid &= diagnostic_prototypes;
                    #[cfg(test)]
                    if !valid {
                        eprintln!("FIRST_USE_DATA_NATIVE test={:?} node={:p} inputs={} children={} direct={} current_inputs={} classes={} prototypes={}",
                            std::thread::current().name(), Rc::as_ptr(node), node.inputs.len(), diagnostic_children, diagnostic_direct,
                            diagnostic_inputs, diagnostic_classes, diagnostic_prototypes);
                    }
                }
                Evidence::Source(root) => {
                    graph.session.check_value_owner(root.actual().value())?;
                    valid &= root.expected.owner.live(graph.session)?;
                    valid &= *self
                        .checked
                        .get(&root.actual().identity(), &mut Meter(graph.session))?
                        .ok_or_else(|| invalid("Missing first-use source data check"))?;
                    valid &= root.expected.witness.matches_logical_source(
                        graph.session,
                        graph.bridge,
                        root.actual().value(),
                    )?;
                }
            }
            self.checked.insert(key, valid, &mut Meter(graph.session))?;
        }
        self.checked
            .get(&evidence.identity(), &mut Meter(graph.session))?
            .copied()
            .ok_or_else(|| invalid("Missing first-use checked result"))
    }
}
fn source_children(session: &mut Session, actual: &Evidence) -> Result<Vec<Key>> {
    let mut pending = Vec::new();
    populate(session, &mut pending, std::iter::once(actual))?;
    let mut keys = Vec::new();
    let mut seen = TryOrderedMap::<(u8, usize), bool>::default();
    while let Some(input) = pending.pop() {
        session.charge_host_work(2)?;
        let pointer = input.identity();
        if seen.get(&pointer, &mut Meter(session))?.is_some() {
            continue;
        }
        seen.insert(pointer, true, &mut Meter(session))?;
        match input {
            Evidence::Source(root) => {
                let mut duplicate = false;
                for old in &keys {
                    session.charge_host_work(key_comparison_work(old, &root.expected.key))?;
                    duplicate |= *old == root.expected.key;
                }
                if !duplicate {
                    reserve(session, &mut keys, 1)?;
                    // Key contains owned Global text on the ReaderSource arm.
                    let text = match &root.expected.key {
                        Key::Declaration(global, _)
                        | Key::DeclarationField(global, _, _)
                        | Key::ReaderSource(global, _, _) => global
                            .namespace()
                            .len()
                            .checked_add(global.name().len())
                            .ok_or_else(|| invalid("Data child Global clone extent overflow"))?,
                        _ => 0,
                    };
                    let bytes = std::mem::size_of::<Key>()
                        .checked_mul(2)
                        .and_then(|n| text.checked_mul(2).and_then(|text| n.checked_add(text)))
                        .ok_or_else(|| invalid("Data child Key clone prepayment overflow"))?;
                    session.charge_host_work(bytes as u64 + 4)?;
                    keys.push(root.expected.key.clone());
                }
            }
            Evidence::Native(node) => populate(session, &mut pending, node.inputs.iter().rev())?,
        }
    }
    Ok(keys)
}
impl Root {
    fn checked_current(
        self: &Rc<Self>,
        graph: &mut AnalysisGraph<'_>,
        expected: &Rc<Expectation>,
        audit: &mut Audit,
    ) -> Result<bool> {
        graph
            .session
            .charge_host_work(key_comparison_work(&self.expected.key, &expected.key))?;
        let mut valid = self.expected.key == expected.key;
        let trace_key_equal = valid;
        let trace_owner_live = expected.owner.live(graph.session)?;
        valid &= trace_owner_live;
        let trace_children_equal = same_child_keys(graph.session, &self.expected.children, &expected.children)?;
        valid &= trace_children_equal;
        // The current expected witness remains independent of retained native
        // evidence; no same-text/current-holder shortcut substitutes for it.
        #[cfg(test)]
        let diagnostic_binding = valid;
        let diagnostic_logical = expected.witness.matches_logical_source(
            graph.session,
            graph.bridge,
            self.actual().value(),
        )?;
        valid &= diagnostic_logical;
        graph.session.charge_host_work(2)?;
        let evidence = Evidence::Source(self.clone());
        let diagnostic_audit = audit.check(graph, &evidence)?;
        valid &= diagnostic_audit;
        reader_plan::trace_declaration_key(&expected.key, "current-check", format_args!(
            "key_equal={} owner_live={} children_equal={} old_children={} current_children={} logical={} audit={} valid={}",
            trace_key_equal, trace_owner_live, trace_children_equal, self.expected.children.len(),
            expected.children.len(), diagnostic_logical, diagnostic_audit, valid));
        reader_plan::trace_declaration_key(&self.expected.key, "checked-retained-owner", format_args!("key_equal={}", trace_key_equal));
        #[cfg(test)]
        if !valid {
            eprintln!("FIRST_USE_DATA_ROOT binding={} logical={} audit={} children={}",
                diagnostic_binding, diagnostic_logical, diagnostic_audit, expected.children.len());
        }
        Ok(valid)
    }
}

enum Scope {
    Native(FormBridge),
    Source(usize),
}
struct Build<'a> {
    plan: &'a reader_plan::NativeReaderPlan,
    retained: &'a [Rc<Root>],
    selected: Vec<Option<Rc<Root>>>,
}
impl Build<'_> {
    fn source_id(&self, graph: &mut AnalysisGraph<'_>, id: Value) -> Result<Option<usize>> {
        let Some(index) = graph.source_roots.index(graph.session, id)? else {
            return Ok(None);
        };
        graph.session.charge_host_work(1)?;
        Ok(self.plan.expected[index].as_ref().map(|_| index))
    }
    fn expected(&self, id: usize) -> Result<&Rc<Expectation>> {
        self.plan
            .expected
            .get(id)
            .and_then(Option::as_ref)
            .ok_or_else(|| invalid("Missing current first-use expectation"))
    }
    fn observation(session: &mut Session, evidence: &Evidence) -> Result<Observed<Evidence>> {
        session.charge_host_work(2)?;
        Ok(Observed {
            value: evidence.value().clone(),
            evidence: evidence.retained(session)?,
        })
    }
    // Stage ALL alias/native consistency checks before making any scratch
    // binding. No guest callbacks run and a failed candidate causes ordinary
    // first-use construction, never error-triggered replay.
    fn bindings(
        &self,
        graph: &mut AnalysisGraph<'_>,
        current: &Context<Evidence>,
        root: &Rc<Root>,
    ) -> Result<Option<Vec<(Value, Observed<Evidence>)>>> {
        let mut pending = Vec::new();
        graph.session.charge_host_work(2)?;
        populate(
            graph.session,
            &mut pending,
            std::iter::once(Evidence::Source(root.clone())),
        )?;
        let mut seen = TryOrderedMap::<(u8, usize), bool>::default();
        let mut bindings: Vec<(Value, Observed<Evidence>)> = Vec::new();
        while let Some(evidence) = pending.pop() {
            graph.session.charge_host_work(2)?;
            let pointer = evidence.identity();
            if seen.get(&pointer, &mut Meter(graph.session))?.is_some() {
                continue;
            }
            seen.insert(pointer, true, &mut Meter(graph.session))?;
            match &evidence {
                Evidence::Native(node) => {
                    reserve(graph.session, &mut pending, node.inputs.len())?;
                    for child in node.inputs.iter().rev() {
                        graph.session.charge_host_work(1)?;
                        pending.push(child.retained(graph.session)?);
                    }
                }
                Evidence::Source(source) => {
                    let mut matched = false;
                    for &id in &self.plan.order {
                        let expected = self.expected(id)?;
                        graph.session.charge_host_work(key_comparison_work(
                            &source.expected.key,
                            &expected.key,
                        ))?;
                        if source.expected.key != expected.key {
                            continue;
                        }
                        matched = true;
                        if !same_child_keys(
                            graph.session,
                            &source.expected.children,
                            &expected.children,
                        )? {
                            reader_plan::trace_declaration_key(&root.expected.key, "bindings-decline", format_args!("reason=descendant-child-keys"));
                            return Ok(None);
                        }
                        for (native, selected) in [
                            (graph.source_roots.roots[id].alias, &evidence),
                            (graph.source_roots.roots[id].native, source.actual()),
                        ] {
                            graph.session.charge_host_work(2)?;
                            if let Some(old) = current.cached(native) {
                                if !graph
                                    .session
                                    .values_identical(&old.value, selected.value())?
                                {
                                    reader_plan::trace_declaration_key(&root.expected.key, "bindings-decline", format_args!("reason=already-built-native-or-alias"));
                                    return Ok(None);
                                }
                            }
                            for (old_id, old) in &bindings {
                                graph.session.charge_host_work(2)?;
                                if *old_id == native
                                    && !graph
                                        .session
                                        .values_identical(&old.value, selected.value())?
                                {
                                    reader_plan::trace_declaration_key(&root.expected.key, "bindings-decline", format_args!("reason=staged-native-or-alias-conflict"));
                                    return Ok(None);
                                }
                            }
                            reserve(graph.session, &mut bindings, 1)?;
                            bindings.push((native, Self::observation(graph.session, selected)?));
                        }
                        break;
                    }
                    if !matched {
                        reader_plan::trace_declaration_key(&root.expected.key, "bindings-decline", format_args!("reason=descendant-owner-absent"));
                        return Ok(None);
                    }
                    let actual = source.actual().retained(graph.session)?;
                    populate(graph.session, &mut pending, std::iter::once(actual))?;
                }
            }
        }
        reader_plan::trace_declaration_key(&root.expected.key, "bindings-ok", format_args!("count={}", bindings.len()));
        Ok(Some(bindings))
    }
}
impl Observer for Build<'_> {
    type Evidence = Evidence;
    type Scope = Scope;
    fn retain(
        &mut self,
        session: &mut Session,
        source: &Observed<Evidence>,
    ) -> Result<Observed<Evidence>> {
        Self::observation(session, &source.evidence)
    }
    fn enter(
        &mut self,
        graph: &mut AnalysisGraph<'_>,
        id: Value,
        cached: Option<&Observed<Evidence>>,
        context: &Context<Evidence>,
    ) -> Result<Entry<Evidence, Scope>> {
        if let Some(cached) = cached {
            // Source aliases retain identity but not an unchecked valid bit.
            if let Evidence::Source(root) = &cached.evidence {
                let source = self
                    .source_id(graph, id)?
                    .ok_or_else(|| invalid("Source evidence at non-source alias"))?;
                let mut audit = Audit::new(graph.session)?;
                if !root.checked_current(graph, self.expected(source)?, &mut audit)? {
                    return Err(invalid(
                        "First-use source changed after selection; no construction replay",
                    ));
                }
                self.selected[source] = Some(root.clone());
            }
            return Ok(Entry::Selected {
                value: self.retain(graph.session, cached)?,
                bindings: Vec::new(),
            });
        }
        if let Some(source) = self.source_id(graph, id)? {
            let expected = self.expected(source)?;
            reader_plan::trace_declaration_key(&expected.key, "parent-enter", format_args!("source={} native={}", source, graph.source_roots.roots[source].native));
            // A bare native value already built by an earlier inline sibling
            // wins. A retained same-content object must not replace it.
            if context
                .cached(graph.source_roots.roots[source].native)
                .is_none()
            {
                for root in self.retained {
                    graph
                        .session
                        .charge_host_work(key_comparison_work(&root.expected.key, &expected.key))?;
                    if reader_plan::declaration_key_traced(&expected.key) {
                        reader_plan::trace_declaration_key(&root.expected.key, "retained-candidate", format_args!("key_equal={}", root.expected.key == expected.key));
                    }
                    if root.expected.key != expected.key {
                        continue;
                    }
                    let mut audit = Audit::new(graph.session)?;
                    if root.checked_current(graph, expected, &mut audit)? {
                        if let Some(bindings) = self.bindings(graph, context, root)? {
                            graph.session.charge_host_work(2)?;
                            reader_plan::trace_declaration_key(&expected.key, "parent-selected", format_args!("source={}", source));
                            let selected = Evidence::Source(root.clone());
                            self.selected[source] = Some(root.clone());
                            return Ok(Entry::Selected {
                                value: Self::observation(graph.session, &selected)?,
                                bindings,
                            });
                        }
                    }
                }
            }
            if reader_plan::declaration_key_traced(&expected.key) {
                reader_plan::trace_declaration_key(&expected.key, "parent-fresh", format_args!(
                    "bare_native_already_built={}", context.cached(graph.source_roots.roots[source].native).is_some()));
            }
            return Ok(Entry::Visit(Scope::Source(source)));
        }
        // Records are fresh native factory observations. No old guest object is
        // quoted/read back and recaptured as proof. This fork captures original
        // bridge inputs; produce_native_recipe retains the original factories.
        Ok(Entry::Visit(Scope::Native(
            graph.bridge.recording(graph.session)?,
        )))
    }
    fn construct(
        &mut self,
        graph: &mut AnalysisGraph<'_>,
        scope: Scope,
        id: Value,
        inputs: Vec<Observed<Evidence>>,
    ) -> Result<Observed<Evidence>> {
        let mut values = Vec::new();
        reserve(graph.session, &mut values, inputs.len())?;
        for input in &inputs {
            graph.session.charge_host_work(2)?;
            values.push(input.value.clone());
        }
        match scope {
            Scope::Source(source) => {
                // A genuine captured boundary is exactly an Alias, not a new
                // authority constructor or a different materialization call.
                if !matches!(graph.recipes[id].as_ref(), Some(Recipe::Alias(_)))
                    || inputs.len() != 1
                {
                    return Err(invalid("Invalid first-use source boundary"));
                }
                let value = graph.produce_native_recipe(id, graph.bridge, &values)?;
                let actual = inputs.into_iter().next().unwrap().evidence;
                let expected = self.expected(source)?;
                let children = source_children(graph.session, &actual)?;
                if !same_child_keys(graph.session, &children, &expected.children)? {
                    return Err(invalid("First-use source native child order changed"));
                }
                let root = Root::allocate(graph.session, expected, actual)?;
                let mut audit = Audit::new(graph.session)?;
                if !root.checked_current(graph, expected, &mut audit)? {
                    #[cfg(test)]
                    eprintln!("FIRST_USE_DATA_FRESH_FAILURE source={} owner={} alias={} native={}",
                        source, graph.source_roots.roots[source].owner.kind(), id,
                        graph.source_roots.roots[source].native);
                    return Err(invalid(
                        "Fresh first-use source failed complete DATA checks",
                    ));
                }
                graph.session.charge_host_work(2)?;
                self.selected[source] = Some(root.clone());
                Ok(Observed {
                    value,
                    evidence: Evidence::Source(root),
                })
            }
            Scope::Native(recording) => {
                let value = graph.produce_native_recipe(id, &recording, &values)?;
                let records = recording.into_records()?;
                // Alias forwards its one exact input without a factory call.
                // Its empty recording cannot authenticate a newly invented
                // Native root. Preserve the original input's complete evidence;
                // all later source/native and post-callback checks still run.
                graph.session.charge_host_work(6)?; // recipe/mode/extent controls
                if matches!(graph.recipes[id].as_ref(), Some(Recipe::Alias(_))) {
                    if inputs.len() != 1 || records.len() != 0 {
                        return Err(invalid("Transparent DATA alias has unexpected construction"));
                    }
                    // Retain the existing Native payload lifecycle tariff even
                    // though no new Native is allocated. In particular original
                    // recording/class-input retirement does not become unpaid.
                    graph.session.charge_host_work(
                        (4 * std::mem::size_of::<Native>()
                            + 2 * std::mem::size_of::<usize>() + 8) as u64,
                    )?;
                    graph.session.charge_host_work(3)?; // extraction/move/retirement
                    let actual = inputs.into_iter().next().unwrap();
                    return Ok(Observed { value, evidence: actual.evidence });
                }
                let mut children = Vec::new();
                reserve(graph.session, &mut children, inputs.len())?;
                graph.session.charge_host_work(inputs.len() as u64)?;
                children.extend(inputs.into_iter().map(|input| input.evidence));
                let native = Native::allocate(graph.session, &value, records, children)?;
                #[cfg(test)]
                eprintln!("RETAINED_DATA_NODE test={:?} node={:p} recipe={} kind={:?} inputs={} records={}",
                    std::thread::current().name(), Rc::as_ptr(&native), id,
                    match graph.recipes[id].as_ref().unwrap() {
                        Recipe::Metadata { .. } => "Metadata", Recipe::Form(_) => "Form",
                        Recipe::Identifier { .. } => "Identifier", Recipe::Map(_) => "Map",
                        Recipe::Vector(_) => "Vector", other => {
                            eprintln!("RETAINED_DATA_OTHER recipe={} discriminant={:?}", id, std::mem::discriminant(other)); "Other"
                        } },
                    native.inputs.len(), native.records.len());
                Ok(Observed {
                    value,
                    evidence: Evidence::Native(native),
                })
            }
        }
    }
}
impl AnalysisGraph<'_> {
    /// The only activated controller: one native first-use context. Boundary
    /// plan.order is used for native proof inventory and FINAL readonly closure,
    /// never to schedule a guest constructor ahead of an inline sibling.
    pub(in crate::portable_macro_graph) fn materialize_reader_controller(
        &mut self,
        root: Value,
        cache: &mut RootCache,
    ) -> Result<Option<SessionValue>> {
        reader_plan::trace_declarations(self, "controller-enter", format_args!("cache={:p} retained={}", cache, cache.data.len()));
        let _request = self.session.operation_budget_scope()?;
        self.session.charge_host_work(2)?;
        if !self.capture_roots || !self.direct_reader_roots {
            return Ok(None);
        }
        if !self.session.source_controller_ready()? {
            reader_plan::trace_declarations(self, "controller-decline", format_args!("reason=session-not-ready"));
            return Ok(None);
        }
        self.finish_recipes()?;
        let root = self.reader_request_export(root)?;
        let Some(plan) = self.admit_first_use_data_controller(root)? else {
            reader_plan::trace_declarations(self, "ordinary-fallback", format_args!("reason=admission-declined"));
            return Ok(None);
        };
        let mut selected = Vec::new();
        reserve(self.session, &mut selected, plan.layout.len())?;
        self.session.charge_host_work(plan.layout.len() as u64)?;
        selected.resize_with(plan.layout.len(), || None);
        let mut build = Build {
            plan: &plan,
            retained: &cache.data,
            selected,
        };
        let mut context = Context::new(self, root)?;
        let result = context.walk(self, root, &mut build)?;
        // No more constructors/callbacks after this point. Audit reuse is valid
        // only within this closed pass; it is newly allocated for each request.
        let mut audit = Audit::new(self.session)?;
        if !audit.check(self, &result.evidence)? {
            return Err(invalid(
                "Source data changed before whole-build publication",
            ));
        }
        let mut updates = Vec::new();
        reserve(self.session, &mut updates, plan.order.len())?;
        for &id in &plan.order {
            self.session.charge_host_work(2)?;
            // A reused outer value can cut off descendants, whose original
            // observations still appear in the selected source evidence DAG.
            if let Some(source) = build.selected[id].as_ref() {
                if !source.checked_current(self, build.expected(id)?, &mut audit)? {
                    return Err(invalid("Selected source invalidated by a later callback"));
                }
                if !matches!(source.expected.key, Key::Request(_)) {
                    self.session.charge_host_work(2)?;
                    updates.push(source.clone());
                }
            }
        }
        // Keep current retained descendants even if a reused parent skipped
        // their aliases. Collection walks evidence only; no guest work occurs.
        collect_roots(self.session, &result.evidence, &mut updates)?;
        let mut replacement: Vec<Rc<Root>> = Vec::new();
        reserve(
            self.session,
            &mut replacement,
            cache
                .data
                .len()
                .checked_add(updates.len())
                .ok_or_else(|| invalid("Data identity cache extent overflow"))?,
        )?;
        for old in &cache.data {
            let mut overwritten = false;
            for current in &updates {
                self.session.charge_host_work(key_comparison_work(
                    &old.expected.key,
                    &current.expected.key,
                ))?;
                overwritten |= old.expected.key == current.expected.key;
            }
            if !overwritten {
                self.session.charge_host_work(2)?;
                let retained = Evidence::Source(old.clone());
                if leases_live(self.session, &retained)? {
                    self.session.charge_host_work(3)?;
                    replacement.push(old.clone());
                }
            }
        }
        for current in updates {
            let mut duplicate = false;
            for old in &replacement {
                self.session.charge_host_work(key_comparison_work(
                    &old.expected.key,
                    &current.expected.key,
                ))?;
                if old.expected.key == current.expected.key {
                    duplicate = true;
                    if !self
                        .session
                        .values_identical(old.actual().value(), current.actual().value())?
                    {
                        return Err(invalid("Conflicting source identity updates"));
                    }
                }
            }
            if !duplicate {
                replacement.push(current);
            }
        }
        self.session.charge_host_work(cache.data.len() as u64 + 1)?;
        let value = result.value;
        cache.data = replacement; // ONE final infallible identity-cache commit
        reader_plan::trace_declarations(self, "publication-complete", format_args!("cache={:p} retained={}", cache, cache.data.len()));
        Ok(Some(value))
    }
}
fn collect_roots(
    session: &mut Session,
    evidence: &Evidence,
    output: &mut Vec<Rc<Root>>,
) -> Result<()> {
    let mut pending = Vec::new();
    populate(session, &mut pending, std::iter::once(evidence))?;
    let mut seen = TryOrderedMap::<(u8, usize), bool>::default();
    while let Some(current) = pending.pop() {
        session.charge_host_work(2)?;
        if seen
            .get(&current.identity(), &mut Meter(session))?
            .is_some()
        {
            continue;
        }
        seen.insert(current.identity(), true, &mut Meter(session))?;
        match current {
            Evidence::Native(node) => populate(session, &mut pending, node.inputs.iter().rev())?,
            Evidence::Source(root) => {
                if !matches!(root.expected.key, Key::Request(_)) {
                    reserve(session, output, 1)?;
                    session.charge_host_work(2)?;
                    output.push(root.clone());
                }
                populate(session, &mut pending, std::iter::once(root.actual()))?;
            }
        }
    }
    Ok(())
}
fn leases_live(session: &mut Session, evidence: &Evidence) -> Result<bool> {
    let mut roots = Vec::new();
    collect_roots(session, evidence, &mut roots)?;
    let mut valid = true;
    for root in roots {
        valid &= root.expected.owner.live(session)?;
    }
    Ok(valid)
}

#[cfg(test)]
#[path = "first_use_data/tests.rs"]
mod tests;

#[cfg(test)]
#[path = "first_use_data/lifecycle_tests.rs"]
mod lifecycle_tests;

#[cfg(test)]
#[path = "first_use_data/original_inputs_tests.rs"]
mod original_inputs_tests;
