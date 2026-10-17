//! Independent logical reader-data expectations, never a storage certificate.
//!
//! FQN consistency and nil source-set backing state are checked separately from
//! decoded logical values. This layer still does not authenticate hash/UID
//! fields, properties, complete representation/storage or alias identity. Those need separate checked
//! storage/receipt validation before any retained candidate can be reused.
use super::{
    Result, preserve_source_error,
    raw_keys::{Interner, Plan},
    validate_form_data,
};
use crate::{
    portable_macro_data::FormBridge,
    portable_session::{Session, SessionValue},
};
use suss_reader::forms::{Form, Kind};

pub(crate) struct LogicalSourceWitness {
    // Built only from immutable native source; never refreshed from a guest read.
    expected: Form,
    small_unordered: bool,
}

fn allocation<T>(session: &mut Session, count: usize) -> Result<()> {
    session.charge_host_work((count as u64).saturating_mul(std::mem::size_of::<T>() as u64))
}
// Normal planned disposal is metered and iterative. Error unwinding still
// uses ordinary owned Rust cleanup; no witness is published on that path.
pub(super) fn discard_form(session: &mut Session, value: Form) -> Result<()> {
    fn push(session: &mut Session, pending: &mut Vec<Form>, value: Form) -> Result<()> {
        session.charge_host_work(1)?;
        if pending.len() == pending.capacity() {
            let count = pending
                .capacity()
                .checked_mul(2)
                .ok_or_else(|| {
                    crate::portable_session::SessionError::Host(wasmtime::Error::msg(
                        "Logical discard capacity overflow",
                    ))
                })?
                .max(4);
            allocation::<Form>(session, count)?;
            allocation::<Form>(session, pending.len())?;
            pending
                .try_reserve_exact(count - pending.len())
                .map_err(|_| {
                    crate::portable_session::SessionError::Host(wasmtime::Error::msg(
                        "Logical discard allocation failed",
                    ))
                })?;
        }
        pending.push(value);
        Ok(())
    }
    let mut pending = Vec::new();
    push(session, &mut pending, value)?;
    while let Some(value) = pending.pop() {
        session.charge_host_work(1)?;
        let (_, metadata, kind) = value.into_parts();
        for value in metadata {
            push(session, &mut pending, value)?;
        }
        match kind {
            Kind::List(v)
            | Kind::Vector(v)
            | Kind::Map(v)
            | Kind::Set(v)
            | Kind::Conditional(v)
            | Kind::SplicingConditional(v) => {
                for value in v {
                    push(session, &mut pending, value)?;
                }
            }
            Kind::Discard(value) => push(session, &mut pending, *value)?,
            Kind::Prefix { operator, target } => {
                push(session, &mut pending, *operator)?;
                push(session, &mut pending, *target)?;
            }
            _ => {}
        }
    }
    Ok(())
}
fn normalize(session: &mut Session, mut form: Form) -> Result<Form> {
    stacker::maybe_grow(256 * 1024, 4 * 1024 * 1024, || {
        session.charge_host_work(1)?;
        let metadata = if form.metadata.is_empty() {
            None
        } else {
            Some(Form {
                span: form.span.clone(),
                metadata: vec![],
                kind: Kind::Map(preserve_source_error(
                    form.span.clone(),
                    &mut |work| session.charge_host_work(work),
                    |charge| {
                        crate::portable::hir::reader_metadata_pairs_metered(&form, &mut |work| {
                            charge(work)
                        })
                    },
                )?),
            })
        };
        for old in std::mem::take(&mut form.metadata) {
            discard_form(session, old)?;
        }
        form.kind = match form.take_kind() {
            Kind::List(values) => Kind::List(normalize_children(session, values)?),
            Kind::Vector(values) => Kind::Vector(normalize_children(session, values)?),
            Kind::Map(values) => Kind::Map(normalize_children(session, values)?),
            Kind::Set(values) => Kind::Set(normalize_children(session, values)?),
            other => other,
        };
        if let Some(metadata) = metadata {
            allocation::<Form>(session, 1)?;
            form.metadata.push(normalize(session, metadata)?);
        }
        Ok(form)
    })
}
fn normalize_children(session: &mut Session, values: Vec<Form>) -> Result<Vec<Form>> {
    allocation::<Form>(session, values.len())?;
    values
        .into_iter()
        .map(|value| normalize(session, value))
        .collect()
}
fn text_equal<T: PartialEq>(session: &mut Session, left: &[T], right: &[T]) -> Result<bool> {
    session.charge_host_work(1)?;
    if left.len() != right.len() {
        return Ok(false);
    }
    for (left, right) in left.chunks(256).zip(right.chunks(256)) {
        session.charge_host_work((left.len() + right.len()) as u64)?;
        if left != right {
            return Ok(false);
        }
    }
    Ok(true)
}
fn keyword_equal(
    session: &mut Session,
    a: &suss_reader::Keyword,
    b: &suss_reader::Keyword,
) -> Result<bool> {
    session.charge_host_work(1)?;
    let namespace = match (&a.namespace, &b.namespace) {
        (None, None) => true,
        (Some(a), Some(b)) => text_equal(session, a.as_bytes(), b.as_bytes())?,
        _ => false,
    };
    Ok(namespace && text_equal(session, a.name.as_bytes(), b.name.as_bytes())?)
}
fn scalar_key(form: &Form, map: bool) -> bool {
    match form.kind {
        Kind::String(_) | Kind::Keyword(_) => true,
        Kind::Nil | Kind::Bool(_) => !map,
        Kind::Number(value) => !map && value.is_finite(),
        _ => false,
    }
}
fn key_equal(session: &mut Session, a: &Form, b: &Form) -> Result<bool> {
    session.charge_host_work(1)?;
    Ok(match (&a.kind, &b.kind) {
        (Kind::Nil, Kind::Nil) => true,
        (Kind::Bool(a), Kind::Bool(b)) => a == b,
        // Only finite set keys reach here. Signed zeros are duplicate keys,
        // although the logical source value check retains their exact bits.
        (Kind::Number(a), Kind::Number(b)) => a == b,
        (Kind::String(a), Kind::String(b)) => text_equal(session, a, b)?,
        (Kind::Keyword(a), Kind::Keyword(b)) => keyword_equal(session, a, b)?,
        _ => false,
    })
}
fn validate_logical(session: &mut Session, root: &Form) -> Result<()> {
    allocation::<&Form>(session, 1)?;
    let mut pending = vec![root];
    while let Some(form) = pending.pop() {
        session.charge_host_work(1)?;
        let children: &[Form] = match &form.kind {
            Kind::Map(values) if !values.len().is_multiple_of(2) => {
                return Err(crate::portable_session::SessionError::Host(
                    wasmtime::Error::msg("Logical source map requires paired entries"),
                ));
            }
            Kind::List(v) | Kind::Vector(v) | Kind::Map(v) | Kind::Set(v) => v,
            Kind::Nil
            | Kind::Bool(_)
            | Kind::Number(_)
            | Kind::String(_)
            | Kind::Symbol(_)
            | Kind::Keyword(_) => &[],
            _ => {
                return Err(crate::portable_session::SessionError::Host(
                    wasmtime::Error::msg("Logical source requires resolved reader data"),
                ));
            }
        };
        if !form.metadata.is_empty()
            && (!matches!(
                form.kind,
                Kind::Symbol(_) | Kind::List(_) | Kind::Vector(_) | Kind::Map(_) | Kind::Set(_)
            ) || form.metadata.len() != 1
                || !matches!(form.metadata[0].kind, Kind::Map(_)))
        {
            return Err(crate::portable_session::SessionError::Host(
                wasmtime::Error::msg(
                    "Logical source metadata must be one map on a symbol or collection",
                ),
            ));
        }
        let extra = children.len().saturating_add(form.metadata.len());
        session.charge_host_work(extra as u64)?;
        if extra > pending.capacity() - pending.len() {
            allocation::<&Form>(session, pending.len().saturating_add(extra))?;
            allocation::<&Form>(session, pending.len())?;
            pending.reserve_exact(extra);
        }
        pending.extend(children);
        pending.extend(&form.metadata);
    }
    Ok(())
}
fn unambiguous(session: &mut Session, form: &Form) -> Result<bool> {
    allocation::<&Form>(session, 1)?;
    let mut pending = vec![form];
    while let Some(form) = pending.pop() {
        session.charge_host_work(1)?;
        if let Kind::Map(values) | Kind::Set(values) = &form.kind {
            let map = matches!(form.kind, Kind::Map(_));
            let stride = if map { 2 } else { 1 };
            for index in (0..values.len()).step_by(stride) {
                session.charge_host_work(1)?;
                if !scalar_key(&values[index], map) {
                    return Ok(false);
                }
                for previous in (0..index).step_by(stride) {
                    if key_equal(session, &values[previous], &values[index])? {
                        return Ok(false);
                    }
                }
            }
        }
        let children: &[Form] = match &form.kind {
            Kind::List(v) | Kind::Vector(v) | Kind::Map(v) | Kind::Set(v) => v,
            _ => &[],
        };
        // Reserve and charge before publishing pending traversal entries.
        let extra = children.len().saturating_add(form.metadata.len());
        session.charge_host_work(extra as u64)?;
        if extra > pending.capacity() - pending.len() {
            allocation::<&Form>(session, pending.len().saturating_add(extra))?;
            allocation::<&Form>(session, pending.len())?;
            pending.reserve_exact(extra);
        }
        pending.extend(children);
        pending.extend(&form.metadata);
    }
    Ok(true)
}

// Expected forms have passed validate_logical and unambiguous. This separate
// charged admission walk bounds quadratic key matching without copying forms.
fn small_unordered(session: &mut Session, root: &Form) -> Result<bool> {
    let mut pending = Vec::new();
    super::reuse::reserve(session, &mut pending, 1)?;
    session.charge_host_work(1)?;
    pending.push(root);
    while let Some(form) = pending.pop() {
        // Removal, kind/length inspection and threshold checks.
        session.charge_host_work(4)?;
        let children: &[Form] = match &form.kind {
            Kind::Map(v) => {
                if v.len() / 2 > 16 { return Ok(false); }
                v
            }
            Kind::Set(v) => {
                if v.len() > 16 { return Ok(false); }
                v
            }
            Kind::List(v) | Kind::Vector(v) => v,
            _ => &[],
        };
        for child in children.iter().chain(form.metadata.iter()) {
            super::reuse::reserve(session, &mut pending, 1)?;
            session.charge_host_work(1)?;
            pending.push(child);
        }
    }
    Ok(true)
}

fn paired_logical_equal(session: &mut Session, expected: &Form, actual: &Form) -> Result<bool> {
    fn push<'a, 'b>(session: &mut Session,
        pending: &mut Vec<(&'a Form, &'b Form)>, a: &'a Form, b: &'b Form,
    ) -> Result<()> {
        super::reuse::reserve(session, pending, 1)?;
        // Two-reference staging and publication; reserve bills separately.
        session.charge_host_work(3)?;
        pending.push((a, b));
        Ok(())
    }
    fn namespace_equal(session: &mut Session, a: &Option<String>, b: &Option<String>) -> Result<bool> {
        session.charge_host_work(1)?;
        match (a, b) {
            (None, None) => Ok(true),
            (Some(a), Some(b)) => text_equal(session, a.as_bytes(), b.as_bytes()),
            _ => Ok(false),
        }
    }
    let mut pending = Vec::new();
    push(session, &mut pending, expected, actual)?;
    while let Some((a, b)) = pending.pop() {
        // Fixed pair/header reads, kind dispatch and scalar checks. Variable
        // text comparisons, key scans and child pushes are charged separately.
        session.charge_host_work(8)?;
        if a.metadata.len() != b.metadata.len() { return Ok(false); }
        match (&a.kind, &b.kind) {
            (Kind::Nil, Kind::Nil) => {}
            (Kind::Bool(a), Kind::Bool(b)) if a == b => {}
            // key_equal is only a locator: signed zero and NaN payloads retain
            // exact raw-key semantics here, including for set elements.
            (Kind::Number(a), Kind::Number(b)) if a.to_bits() == b.to_bits() => {}
            (Kind::String(a), Kind::String(b)) => {
                if !text_equal(session, a, b)? { return Ok(false); }
            }
            (Kind::Symbol(a), Kind::Symbol(b)) => {
                if !namespace_equal(session, &a.namespace, &b.namespace)?
                    || !text_equal(session, a.name.as_bytes(), b.name.as_bytes())? {
                    return Ok(false);
                }
            }
            (Kind::Keyword(a), Kind::Keyword(b)) => {
                if !keyword_equal(session, a, b)? { return Ok(false); }
            }
            (Kind::List(a), Kind::List(b)) | (Kind::Vector(a), Kind::Vector(b)) => {
                if a.len() != b.len() { return Ok(false); }
                for (a, b) in a.iter().zip(b).rev() { push(session, &mut pending, a, b)?; }
            }
            (Kind::Map(av), Kind::Map(bv)) | (Kind::Set(av), Kind::Set(bv)) => {
                if matches!(&a.kind, Kind::Map(_))
                    && (!av.len().is_multiple_of(2) || !bv.len().is_multiple_of(2)) {
                    return Ok(false);
                }
                if av.len() != bv.len() { return Ok(false); }
                let stride = if matches!(&a.kind, Kind::Map(_)) { 2 } else { 1 };
                // Equal counts + unique expected equivalence keys imply a
                // bijection if every expected key has a match. Thus actual
                // duplicates/missing keys fail without an auxiliary seen vec.
                for i in (0..av.len()).step_by(stride).rev() {
                    session.charge_host_work(1)?;
                    let mut found = None;
                    for j in (0..bv.len()).step_by(stride) {
                        session.charge_host_work(1)?;
                        if key_equal(session, &av[i], &bv[j])? {
                            found = Some(j);
                            break;
                        }
                    }
                    let Some(j) = found else { return Ok(false); };
                    if stride == 2 { push(session, &mut pending, &av[i + 1], &bv[j + 1])?; }
                    push(session, &mut pending, &av[i], &bv[j])?;
                }
            }
            _ => return Ok(false),
        }
        // Metadata vectors are ordered; their map contents still compare
        // unordered when the paired traversal reaches those map nodes.
        for (a, b) in a.metadata.iter().zip(&b.metadata).rev() {
            push(session, &mut pending, a, b)?;
        }
    }
    Ok(true)
}

pub(super) fn normalized_copy(session: &mut Session, source: &Form) -> Result<Form> {
    validate_form_data(source, &mut |units| session.charge_host_work(units))?;
    let copy = preserve_source_error(
        source.span.clone(),
        &mut |work| session.charge_host_work(work),
        |charge| crate::portable::clone_form_metered(source, &mut |work| charge(work)),
    )?;
    normalize(session, copy)
}
// An owned finite observation, never a source/storage authority. Only the
// private callback-free root check retains this across two comparisons.
pub(super) struct ObservedSource {
    actual: Form,
    source_valid: bool,
}
impl ObservedSource {
    pub(super) fn read(session: &mut Session, bridge: &FormBridge, value: &SessionValue) -> Result<Self> {
        let (actual, source_valid) = bridge.read_finite(session, value)?;
        Ok(Self { actual, source_valid })
    }
}
impl LogicalSourceWitness {
    /// None is conservative admission decline, never a successful certificate.
    /// Compound/ambiguous keys still need native portable key normalization.
    pub(crate) fn from_source(session: &mut Session, source: &Form) -> Result<Option<Self>> {
        let _request = session.operation_budget_scope()?;
        let expected = normalized_copy(session, source)?;
        Self::from_normalized_source(session, expected)
    }
    // Native recipe projection has already applied reader normalization and
    // metadata replacement. Preserve metadata on its metadata-map values.
    pub(super) fn from_normalized_source(
        session: &mut Session,
        expected: Form,
    ) -> Result<Option<Self>> {
        let _request = session.operation_budget_scope()?;
        // Structural checks precede conservative admission, so an unsupported
        // key cannot hide an odd map or malformed metadata elsewhere.
        validate_logical(session, &expected)?;
        if !unambiguous(session, &expected)? {
            return Ok(None);
        }
        let small_unordered = small_unordered(session, &expected)?;
        Ok(Some(Self { expected, small_unordered }))
    }
    pub(crate) fn matches_logical_source(
        &self,
        session: &mut Session,
        bridge: &FormBridge,
        value: &SessionValue,
    ) -> Result<bool> {
        let _request = session.operation_budget_scope()?;
        let observed = ObservedSource::read(session, bridge, value)?;
        self.matches_observed(session, &observed)
    }
    pub(super) fn matches_observed(
        &self, session: &mut Session, observed: &ObservedSource,
    ) -> Result<bool> {
        let _request = session.operation_budget_scope()?;
        // This comparison still performs work and must poll even for false
        // source validity: the old second finite read polled here as well.
        session.charge_host_work(1)?;
        let actual = &observed.actual;
        if !observed.source_valid {
            return Ok(false);
        }
        if self.small_unordered {
            return paired_logical_equal(session, &self.expected, actual);
        }
        let mut keys = Interner::default();
        let mut plan = Plan::default();
        let expected = keys.unordered_plan(&self.expected, &mut plan, &mut |work| {
            session.charge_host_work(work)
        })?;
        let expected = plan.nodes[expected].key;
        let actual = keys.unordered_plan(actual, &mut plan, &mut |work| {
            session.charge_host_work(work)
        })?;
        session.charge_host_work(1)?;
        Ok(expected == plan.nodes[actual].key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        portable::{hir::Literal, resolve::Phase},
        portable_session::SessionError,
    };
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
    #[test]
    fn finite_observation_compares_each_witness_and_next_read_observes_mutation() {
        for phase in [Phase::Runtime, Phase::Macro] {
            let mut session = session(phase);
            let bridge = FormBridge::new(&mut session).unwrap();
            let good = LogicalSourceWitness::from_source(&mut session, &source("(1 2)")).unwrap().unwrap();
            let wrong = LogicalSourceWitness::from_source(&mut session, &source("(1 3)")).unwrap().unwrap();
            let value = session.eval("(list 1 2)").unwrap();
            let observed = ObservedSource::read(&mut session, &bridge, &value).unwrap();
            session.collect().unwrap();
            assert!(good.matches_observed(&mut session, &observed).unwrap());
            assert!(!wrong.matches_observed(&mut session, &observed).unwrap());
            assert!(good.matches_observed(&mut session, &observed).unwrap());
            // End the pass before the real guest mutation.
            drop(observed);
            let mutate = session.eval("(fn [x] (set! (.-first x) 9))").unwrap();
            drop(session.invoke(&mutate, &[&value]).unwrap());
            let fresh = ObservedSource::read(&mut session, &bridge, &value).unwrap();
            assert!(!good.matches_observed(&mut session, &fresh).unwrap());
        }
    }
    #[test]
    fn finite_observation_comparison_preserves_fuel_irq_and_retry() {
        let mut session = Session::new_macro().unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        let witness = LogicalSourceWitness::from_source(&mut session, &source("(1 2)")).unwrap().unwrap();
        let value = session.eval("(list 1 2)").unwrap();
        let observed = ObservedSource::read(&mut session, &bridge, &value).unwrap();
        session.with_operation_budget(|session| {
            let remaining = session.operation_fuel_remaining()?;
            session.charge_host_work(remaining)?;
            assert!(matches!(witness.matches_observed(session, &observed), Err(SessionError::Trap(_))));
            Ok(())
        }).unwrap();
        session.with_operation_budget(|session| {
            session.interrupt_handle().interrupt();
            assert!(witness.matches_observed(session, &observed).unwrap_err().is_interrupt());
            Ok(())
        }).unwrap();
        assert!(witness.matches_observed(&mut session, &observed).unwrap());
        let keyword = session.eval(":finite-invalid").unwrap();
        corrupt_fqn(&mut session, &keyword);
        let invalid = ObservedSource::read(&mut session, &bridge, &keyword).unwrap();
        assert!(!witness.matches_observed(&mut session, &invalid).unwrap());
        session.with_operation_budget(|session| {
            session.interrupt_handle().interrupt();
            assert!(witness.matches_observed(session, &invalid).unwrap_err().is_interrupt());
            Ok(())
        }).unwrap();
        session.with_operation_budget(|session| {
            let remaining = session.operation_fuel_remaining()?;
            session.charge_host_work(remaining)?;
            assert!(matches!(witness.matches_observed(session, &invalid), Err(SessionError::Trap(_))));
            Ok(())
        }).unwrap();
        assert!(!witness.matches_observed(&mut session, &invalid).unwrap());
    }
    #[test]
    fn source_facts_compare_unordered_pairs_metadata_and_exact_numeric_bits() {
        for phase in [Phase::Runtime, Phase::Macro] {
            let mut session = session(phase);
            let bridge = FormBridge::new(&mut session).unwrap();
            let witness = LogicalSourceWitness::from_source(
                &mut session,
                &source("^{:m 17} {:a [1 2] :b #{101 102 103}}"),
            )
            .unwrap()
            .unwrap();
            for (text, expected) in [
                ("(with-meta {:b #{103 101 102} :a [1 2]} {:m 17})", true),
                ("(with-meta {:a [1 3] :b #{101 102 103}} {:m 17})", false),
                ("(with-meta {:b [1 2] :a #{101 102 103}} {:m 17})", false),
                ("(with-meta {:a [1 2] :b #{101 102 103}} {:m 18})", false),
            ] {
                let value = session.eval(text).unwrap();
                session.collect().unwrap();
                assert_eq!(
                    witness
                        .matches_logical_source(&mut session, &bridge, &value)
                        .unwrap(),
                    expected,
                    "{phase:?}: {text}"
                );
            }
            for (text, value) in [
                ("[^:flag x]", "[(with-meta 'x {:flag true})]"),
                (
                    "^{:m 17} [^:flag x]",
                    "(with-meta [(with-meta 'x {:flag true})] {:m 17})",
                ),
                ("^{} []", "(with-meta [] {})"),
                ("^{:m 17} ^{:m 18} []", "(with-meta [] {:m 17})"),
            ] {
                let witness = LogicalSourceWitness::from_source(&mut session, &source(text))
                    .unwrap()
                    .unwrap();
                let value = session.eval(value).unwrap();
                assert!(
                    witness
                        .matches_logical_source(&mut session, &bridge, &value)
                        .unwrap(),
                    "{text}"
                );
            }
            let duplicate_witness =
                LogicalSourceWitness::from_source(&mut session, &source("{:a 1 :b 1}"))
                    .unwrap()
                    .unwrap();
            let duplicate = session
                .eval("(def source-dupe {:a 1 :b 1}) (aset (.-arr source-dupe) 2 :a) source-dupe")
                .unwrap();
            assert!(
                !duplicate_witness
                    .matches_logical_source(&mut session, &bridge, &duplicate)
                    .unwrap(),
                "malformed duplicate pairs cannot reuse one expected entry twice"
            );
            for bits in [
                0,
                (-0.0f64).to_bits(),
                0x7ff8_0000_0000_0017,
                0x7ff8_0000_0000_0018,
            ] {
                let mut form = source("0");
                form.kind = Kind::Number(f64::from_bits(bits));
                let witness = LogicalSourceWitness::from_source(&mut session, &form)
                    .unwrap()
                    .unwrap();
                let same = session
                    .data_scalar(&Literal::Number(f64::from_bits(bits)))
                    .unwrap();
                let other = session
                    .data_scalar(&Literal::Number(f64::from_bits(bits ^ 1)))
                    .unwrap();
                assert!(
                    witness
                        .matches_logical_source(&mut session, &bridge, &same)
                        .unwrap()
                );
                assert!(
                    !witness
                        .matches_logical_source(&mut session, &bridge, &other)
                        .unwrap()
                );
            }
            for text in ["{[1] 2}", "#{[1]}", "#{0 -0.0}", "{:a 1 :a 2}"] {
                assert!(
                    LogicalSourceWitness::from_source(&mut session, &source(text))
                        .unwrap()
                        .is_none()
                );
            }
        }
    }
    #[test]
    fn source_facts_reject_callback_poison_that_a_fresh_physical_snapshot_accepts() {
        for phase in [Phase::Runtime, Phase::Macro] {
            let mut session = session(phase);
            let bridge = FormBridge::new(&mut session).unwrap();
            let witness =
                LogicalSourceWitness::from_source(&mut session, &source("{:audit {:x 17}}"))
                    .unwrap()
                    .unwrap();
            session.eval("(def source-root {:audit {:x 17}}) (def saved-source-hash suss.core/hash) (def source-hash-calls 0)").unwrap();
            let root = session.eval("source-root").unwrap();
            assert!(
                witness
                    .matches_logical_source(&mut session, &bridge, &root)
                    .unwrap()
            );
            session.enter_namespace("suss.core").unwrap();
            session.eval("(def hash (fn [key] (do (if (= key 101) (do (aset (.-arr (get user/source-root :audit)) 1 99) (set! user/source-hash-calls (+ user/source-hash-calls 1))) nil) (user/saved-source-hash key))))").unwrap();
            session.enter_namespace("user").unwrap();
            session
                .eval("(set [101 102 103 104 105 106 107 108 109])")
                .unwrap();
            let observed = session
                .eval("[(get (get source-root :audit) :x) source-hash-calls]")
                .unwrap();
            assert_eq!(
                crate::portable_repl::display(&mut session, &observed).unwrap(),
                "[99 1]"
            );
            let poisoned_snapshot = session.capture_storage(&root).unwrap();
            session.collect().unwrap();
            assert!(session.storage_matches(&root, &poisoned_snapshot).unwrap());
            assert!(
                !witness
                    .matches_logical_source(&mut session, &bridge, &root)
                    .unwrap()
            );
            let observed = session.eval("(get (get source-root :audit) :x)").unwrap();
            assert_eq!(
                crate::portable_repl::display(&mut session, &observed).unwrap(),
                "99",
                "validation must not repair old storage"
            );
        }
    }
    #[test]
    fn source_facts_do_not_realize_lazy_data_or_authorize_hash_memos() {
        for phase in [Phase::Runtime, Phase::Macro] {
            let mut session = session(phase);
            let bridge = FormBridge::new(&mut session).unwrap();
            let witness = LogicalSourceWitness::from_source(&mut session, &source("(1 2)"))
                .unwrap()
                .unwrap();
            session
                .eval("(def logical-list (list 1 2)) (def finite-calls 0)")
                .unwrap();
            let value = session.eval("logical-list").unwrap();
            assert!(
                witness
                    .matches_logical_source(&mut session, &bridge, &value)
                    .unwrap()
            );
            session
                .eval("(hash logical-list) (set! (.-__hash logical-list) 999)")
                .unwrap();
            assert!(
                witness
                    .matches_logical_source(&mut session, &bridge, &value)
                    .unwrap(),
                "logical equality cannot certify a forged hash field"
            );
            let lazy = session
                .eval("(new cljs.core/LazySeq nil (fn [] (do (set! finite-calls 1) (list 1 2))) nil nil)")
                .unwrap();
            assert!(
                witness
                    .matches_logical_source(&mut session, &bridge, &lazy)
                    .unwrap_err()
                    .to_string()
                    .contains("Lazy data needs fueled source realization")
            );
            let effects = session.eval("finite-calls").unwrap();
            assert_eq!(
                crate::portable_repl::display(&mut session, &effects).unwrap(),
                "0"
            );
            let cycle = session.eval("(def finite-cycle (list 1 2)) (set! (.-rest finite-cycle) finite-cycle) finite-cycle").unwrap();
            session.collect().unwrap();
            assert!(
                witness
                    .matches_logical_source(&mut session, &bridge, &cycle)
                    .unwrap_err()
                    .to_string()
                    .contains("Cyclic")
            );
            let recovery = session.eval("42").unwrap();
            assert_eq!(
                crate::portable_repl::display(&mut session, &recovery).unwrap(),
                "42"
            );
            let mut other = Session::new().unwrap();
            assert!(matches!(
                witness.matches_logical_source(&mut other, &bridge, &value),
                Err(SessionError::ForeignValue)
            ));
        }
    }
    fn corrupt_fqn(session: &mut Session, value: &SessionValue) {
        let forged = session
            .data_scalar(&Literal::String("forged".encode_utf16().collect()))
            .unwrap();
        session
            .inspect(value, |mut store, value| {
                let object = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
                let fields = object
                    .field(&mut store, 1)?
                    .unwrap_anyref()
                    .unwrap()
                    .as_array(&store)?
                    .unwrap();
                let forged = forged.rooted(&mut store);
                fields.set(&mut store, 2, wasmtime::Val::AnyRef(Some(forged)))
            })
            .unwrap();
    }
    #[test]
    fn source_storage_invariants_reject_forged_identifier_fqn_after_gc() {
        for phase in [Phase::Runtime, Phase::Macro] {
            let mut session = session(phase);
            let bridge = FormBridge::new(&mut session).unwrap();
            for (datum, expression) in [
                ("plain", "'plain"),
                ("/", "'/"),
                ("demo/name", "'demo/name"),
                ("ns😀/δ😀", "'ns😀/δ😀"),
                (":plain", ":plain"),
                (":demo/name", ":demo/name"),
                (":ns😀/δ😀", ":ns😀/δ😀"),
            ] {
                let expected = source(datum);
                let witness = LogicalSourceWitness::from_source(&mut session, &expected)
                    .unwrap()
                    .unwrap();
                let value = session.eval(expression).unwrap();
                assert!(
                    witness
                        .matches_logical_source(&mut session, &bridge, &value)
                        .unwrap(),
                    "{phase:?}: {datum}"
                );
                corrupt_fqn(&mut session, &value);
                session.collect().unwrap();
                let ordinary = bridge.read(&mut session, &value, 0..0).unwrap();
                assert_eq!(
                    ordinary.kind, expected.kind,
                    "logical names alone hide the forged operational field"
                );
                assert!(
                    !witness
                        .matches_logical_source(&mut session, &bridge, &value)
                        .unwrap()
                );
            }
            let symbol = session.eval("'demo/name").unwrap();
            corrupt_fqn(&mut session, &symbol);
            let bad = session.eval("(def source-late-cycle (list 1 2)) (set! (.-rest source-late-cycle) source-late-cycle) source-late-cycle").unwrap();
            let composite = bridge.vector_values(&mut session, &[symbol, bad]).unwrap();
            let witness =
                LogicalSourceWitness::from_source(&mut session, &source("[demo/name (1 2)]"))
                    .unwrap()
                    .unwrap();
            assert!(
                witness
                    .matches_logical_source(&mut session, &bridge, &composite)
                    .unwrap_err()
                    .to_string()
                    .contains("Cyclic"),
                "an earlier invariant mismatch must not hide a later decoded error"
            );
        }
    }
    #[test]
    fn source_storage_invariants_reject_hidden_set_backing_values_and_metadata() {
        for phase in [Phase::Runtime, Phase::Macro] {
            let mut session = session(phase);
            let bridge = FormBridge::new(&mut session).unwrap();
            for (datum, backing, class_name) in [
                ("#{:a :b}", "{:a nil :b nil}", "PersistentArrayMap"),
                (
                    "#{:a :b :c :d :e :f :g :h :i}",
                    "(hash-map :a nil :b nil :c nil :d nil :e nil :f nil :g nil :h nil :i nil)",
                    "PersistentHashMap",
                ),
            ] {
                let expected = source(datum);
                let witness = LogicalSourceWitness::from_source(&mut session, &expected)
                    .unwrap()
                    .unwrap();
                let value = session
                    .eval(&format!(
                        "(new cljs.core/PersistentHashSet nil {backing} nil)"
                    ))
                    .unwrap();
                assert!(
                    witness
                        .matches_logical_source(&mut session, &bridge, &value)
                        .unwrap()
                );
                let backing_value = session.data_fields(&value).unwrap()[1].clone();
                let class = session.eval(&format!("suss.core/{class_name}")).unwrap();
                let expected_descriptor = session.data_descriptor(&class, true).unwrap();
                session
                    .inspect(&backing_value, |mut store, value| {
                        let object = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
                        let actual = object.field(&mut store, 0)?;
                        let expected = expected_descriptor.rooted(&mut store);
                        assert!(wasmtime::Rooted::<wasmtime::AnyRef>::ref_eq(
                            &store,
                            actual.unwrap_anyref().unwrap(),
                            &expected
                        )?);
                        Ok(())
                    })
                    .unwrap();
                for changed in [
                    format!("(assoc {backing} :a 999)"),
                    format!("(with-meta {backing} {{:hidden 17}})"),
                ] {
                    let value = session
                        .eval(&format!(
                            "(new cljs.core/PersistentHashSet nil {changed} nil)"
                        ))
                        .unwrap();
                    session.collect().unwrap();
                    // Ordinary finite set reading still exposes the same keys.
                    let ordinary = bridge.read(&mut session, &value, 0..0).unwrap();
                    let mut keys = Interner::default();
                    let mut plan = Plan::default();
                    let left = keys
                        .unordered_plan(&expected, &mut plan, &mut |work| {
                            session.charge_host_work(work)
                        })
                        .unwrap();
                    let right = keys
                        .unordered_plan(&ordinary, &mut plan, &mut |work| {
                            session.charge_host_work(work)
                        })
                        .unwrap();
                    assert_eq!(plan.nodes[left].key, plan.nodes[right].key);
                    assert!(
                        !witness
                            .matches_logical_source(&mut session, &bridge, &value)
                            .unwrap(),
                        "hidden backing state must not become source truth"
                    );
                }
            }
        }
    }
    #[test]
    fn source_facts_construction_preserves_typed_fuel_and_interrupt_errors() {
        let mut session = session(Phase::Macro);
        let form = source("^{:m [1 2]} {:a [1 2 3]}");
        let cost = session
            .with_operation_budget(|session| {
                let before = session.operation_fuel_remaining()?;
                assert!(LogicalSourceWitness::from_source(session, &form)?.is_some());
                Ok(before - session.operation_fuel_remaining()?)
            })
            .unwrap();
        let allowance = 2 * cost - 1;
        assert!(allowance < crate::portable::DEFAULT_OPERATION_FUEL);
        session.set_operation_fuel(allowance);
        let error = session
            .with_operation_budget(|session| {
                assert!(LogicalSourceWitness::from_source(session, &form)?.is_some());
                LogicalSourceWitness::from_source(session, &form)
            })
            .err()
            .unwrap();
        assert!(
            matches!(error, SessionError::Trap(ref error) if error.downcast_ref::<wasmtime::Trap>() == Some(&wasmtime::Trap::OutOfFuel)),
            "{error:?}"
        );
        assert!(
            LogicalSourceWitness::from_source(&mut session, &form)
                .unwrap()
                .is_some()
        );
        // Force real Store-fuel exhaustion inside each diagnostic-returning API.
        for clone in [false, true] {
            let error = session
                .with_operation_budget(|session| {
                    session.charge_host_work(allowance)?;
                    preserve_source_error(
                        form.span.clone(),
                        &mut |work| session.charge_host_work(work),
                        |charge| {
                            if clone {
                                crate::portable::clone_form_metered(&form, &mut |work| charge(work))
                                    .map(|_| ())
                            } else {
                                crate::portable::hir::reader_metadata_pairs_metered(
                                    &form,
                                    &mut |work| charge(work),
                                )
                                .map(|_| ())
                            }
                        },
                    )
                })
                .unwrap_err();
            assert!(
                matches!(error, SessionError::Trap(ref error) if error.downcast_ref::<wasmtime::Trap>() == Some(&wasmtime::Trap::OutOfFuel)),
                "{error:?}"
            );
            assert!(
                LogicalSourceWitness::from_source(&mut session, &form)
                    .unwrap()
                    .is_some()
            );
        }
        let interrupt = session.host_work_interrupt();
        let error = session
            .with_operation_budget(|session| {
                interrupt.store(true, std::sync::atomic::Ordering::SeqCst);
                LogicalSourceWitness::from_source(session, &form)
            })
            .err()
            .unwrap();
        assert!(
            matches!(error, SessionError::Trap(ref error) if error.downcast_ref::<wasmtime::Trap>() == Some(&wasmtime::Trap::Interrupt)),
            "{error:?}"
        );
        assert!(
            LogicalSourceWitness::from_source(&mut session, &form)
                .unwrap()
                .is_some()
        );
    }
    #[test]
    fn source_facts_share_fuel_and_propagate_interrupt_without_publication() {
        let mut session = session(Phase::Macro);
        let bridge = FormBridge::new(&mut session).unwrap();
        let witness = LogicalSourceWitness::from_source(&mut session, &source("{:a [1 2 3]}"))
            .unwrap()
            .unwrap();
        let value = session.eval("{:a [1 2 3]}").unwrap();
        let cost = session
            .with_operation_budget(|session| {
                let before = session.operation_fuel_remaining()?;
                assert!(witness.matches_logical_source(session, &bridge, &value)?);
                Ok(before - session.operation_fuel_remaining()?)
            })
            .unwrap();
        let allowance = cost.checked_mul(2).unwrap() - 1;
        assert!(allowance < crate::portable::DEFAULT_OPERATION_FUEL);
        session.set_operation_fuel(allowance);
        let error = session
            .with_operation_budget(|session| {
                assert!(witness.matches_logical_source(session, &bridge, &value)?);
                witness.matches_logical_source(session, &bridge, &value)
            })
            .unwrap_err();
        assert!(
            matches!(error, SessionError::Trap(ref error) if error.downcast_ref::<wasmtime::Trap>() == Some(&wasmtime::Trap::OutOfFuel)),
            "{error:?}"
        );
        assert!(
            witness
                .matches_logical_source(&mut session, &bridge, &value)
                .unwrap()
        );
        let interrupt = session.host_work_interrupt();
        let error = session
            .with_operation_budget(|session| {
                interrupt.store(true, std::sync::atomic::Ordering::SeqCst);
                witness.matches_logical_source(session, &bridge, &value)
            })
            .unwrap_err();
        assert!(
            matches!(error, SessionError::Trap(ref error) if error.downcast_ref::<wasmtime::Trap>() == Some(&wasmtime::Trap::Interrupt)),
            "{error:?}"
        );
        assert!(
            witness
                .matches_logical_source(&mut session, &bridge, &value)
                .unwrap()
        );
    }
fn unordered_reference(session: &mut Session, a: &Form, b: &Form) -> Result<bool> {
    let mut keys = Interner::default();
    let mut plan = Plan::default();
    let a = keys.unordered_plan(a, &mut plan, &mut |n| session.charge_host_work(n))?;
    let key = plan.nodes[a].key;
    let b = keys.unordered_plan(b, &mut plan, &mut |n| session.charge_host_work(n))?;
    session.charge_host_work(1)?;
    Ok(key == plan.nodes[b].key)
}
fn native_form(kind: Kind) -> Form { Form { span: 0..0, metadata: vec![], kind } }

#[test]
fn paired_source_comparison_matches_unordered_reference() {
    for phase in [Phase::Runtime, Phase::Macro] {
        let mut session = session(phase);
        let mut cases = Vec::new();
        for (a, b, equal) in [
            ("{:a [1 2] :b #{1 2 3}}", "{:b #{3 1 2} :a [1 2]}", true),
            ("{:a 1 :b 2}", "{:a 1 :a 2}", false),
            ("{:a 1 :b 2}", "{:a 1}", false),
            ("#{1 2}", "#{1 1}", false),
            ("#{1 2}", "#{1}", false),
            ("[1 2]", "[2 1]", false),
            ("(1 2)", "[1 2]", false),
            ("[^{:a 1 :b 2} x]", "[^{:b 2 :a 1} x]", true),
            ("[^{:a 1} x]", "[^{:a 2} x]", false),
            ("foo/name", "bar/name", false),
            ("name", "foo/name", false),
            ("foo/name", "foo/name", true),
        ] {
            let a = normalized_copy(&mut session, &source(a)).unwrap();
            let b = normalized_copy(&mut session, &source(b)).unwrap();
            cases.push((a, b, equal));
        }
        for (a, b, equal) in [
            (0u64, 1u64 << 63, false),
            (0x7ff8_0000_0000_0001, 0x7ff8_0000_0000_0001, true),
            (0x7ff8_0000_0000_0001, 0x7ff8_0000_0000_0002, false),
        ] {
            cases.push((native_form(Kind::Number(f64::from_bits(a))),
                        native_form(Kind::Number(f64::from_bits(b))), equal));
        }
        cases.push((native_form(Kind::Set(vec![native_form(Kind::Number(-0.0))])),
                    native_form(Kind::Set(vec![native_form(Kind::Number(0.0))])), false));
        for (units, equal) in [(vec![0xd800, 0, 0xdc00], true),
                                (vec![0xd801, 0, 0xdc00], false)] {
            cases.push((native_form(Kind::String(vec![0xd800, 0, 0xdc00])),
                        native_form(Kind::String(units)), equal));
        }
        // Key lookup ignores metadata; exact paired comparison must not.
        let a = source("{:a 1}");
        let mut b = a.clone();
        let Kind::Map(entries) = &mut b.kind else { unreachable!() };
        entries[0].metadata.push(source("{:extra 9}"));
        cases.push((a, b, false)); // Native-only malformed actual key metadata.
        // Native metadata-on-metadata map: not expressible by reader-prefix
        // normalization alone. Both witnesses are still genuinely admitted.
        let mut a = source("[1]");
        let mut meta = source("{:a 1 :b 2}");
        meta.metadata.push(source("{:own 7}"));
        a.metadata.push(meta);
        let mut b = a.clone();
        let Kind::Map(entries) = &mut b.metadata[0].kind else { unreachable!() };
        entries.rotate_left(2);
        cases.push((a.clone(), b.clone(), true));
        b.metadata[0].metadata[0] = source("{:own 8}");
        cases.push((a, b, false));
        for (id, (a, b, expected)) in cases.into_iter().enumerate() {
            let witness = LogicalSourceWitness::from_normalized_source(&mut session, a)
                .unwrap().expect("admitted expected");
            assert!(witness.small_unordered);
            let reference = unordered_reference(&mut session, &witness.expected, &b).unwrap();
            let actual = paired_logical_equal(&mut session, &witness.expected, &b).unwrap();
            assert_eq!(reference, expected, "reference case {id}");
            assert_eq!(actual, reference, "paired case {id}");
        }
    }
}

#[test]
fn paired_source_wide_nested_witness_uses_fallback() {
    for phase in [Phase::Runtime, Phase::Macro] {
        let mut session = session(phase);
        let bridge = FormBridge::new(&mut session).unwrap();
        let entries = (0..17).map(|i| format!(":k{i} {i}")).collect::<Vec<_>>().join(" ");
        let text = format!("[{{{entries}}}]");
        let witness = LogicalSourceWitness::from_source(&mut session, &source(&text))
            .unwrap().unwrap();
        assert!(!witness.small_unordered);
        let value = session.eval(&text).unwrap();
        session.collect().unwrap();
        assert!(witness.matches_logical_source(&mut session, &bridge, &value).unwrap());
        let changed = session.eval(&text.replace(":k16 16", ":k16 99")).unwrap();
        assert!(!witness.matches_logical_source(&mut session, &bridge, &changed).unwrap());
        let set_text = format!("[slot #{{{}}}]", (0..17).map(|i| i.to_string()).collect::<Vec<_>>().join(" "));
        let set_witness = LogicalSourceWitness::from_source(&mut session, &source(&set_text)).unwrap().unwrap();
        assert!(!set_witness.small_unordered);
    }
}

#[test]
fn paired_source_comparison_shared_fuel_irq_and_recovery() {
    let mut session = session(Phase::Macro);
    let witness = LogicalSourceWitness::from_source(&mut session,
        &source("^{:own 7} {:a [1 2 3] :b #{4 5 6}}"))
        .unwrap().unwrap();
    assert!(witness.small_unordered);
    let actual = witness.expected.clone();
    let cost = session.with_operation_budget(|session| {
        let before = session.operation_fuel_remaining()?;
        assert!(paired_logical_equal(session, &witness.expected, &actual)?);
        Ok(before - session.operation_fuel_remaining()?)
    }).unwrap();
    let allowance = cost.checked_mul(2).unwrap() - 1;
    session.set_operation_fuel(allowance);
    let error = session.with_operation_budget(|session| {
        assert!(paired_logical_equal(session, &witness.expected, &actual)?);
        paired_logical_equal(session, &witness.expected, &actual)
    }).unwrap_err();
    assert!(matches!(error, SessionError::Trap(ref e)
        if e.downcast_ref::<wasmtime::Trap>() == Some(&wasmtime::Trap::OutOfFuel)));
    // Same allowance succeeds for one complete comparison after failure.
    assert!(session.with_operation_budget(|session|
        paired_logical_equal(session, &witness.expected, &actual)).unwrap());
    let irq = session.host_work_interrupt();
    let error = session.with_operation_budget(|session| {
        irq.store(true, std::sync::atomic::Ordering::SeqCst);
        paired_logical_equal(session, &witness.expected, &actual)
    }).unwrap_err();
    assert!(error.is_interrupt());
    assert!(session.with_operation_budget(|session|
        paired_logical_equal(session, &witness.expected, &actual)).unwrap());
    // Keep existing source_facts_share_fuel_and_propagate_interrupt_without_publication:
    // it exercises the genuine decode+comparison route, not only this helper.
}


}
