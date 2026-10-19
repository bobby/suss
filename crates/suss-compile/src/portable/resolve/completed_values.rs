//! Bounded native completed-value inputs. No predecessor Info, initializer HIR,
//! callable body, method environment or namespace catalog is retained.
//! Policy: pinned analyzer.cljc2092–2172/3630–3645, c4295f3, EPL-1.0.
use super::{DefinitionInfo, Global};
use crate::portable::{Diagnostic, SourceOrigin, clone_form_metered, hir};
use suss_reader::forms::{Form, Kind};
use std::sync::Arc;
type Meter<'a> = dyn FnMut(u64) -> Result<(), Diagnostic> + 'a;

pub(crate) const VALUE_FIELDS: [&str; 22] = ["name", "ns", "private", "dynamic", "doc", "declared",
    "tag", "ret-tag", "fn-var", "variadic?", "max-fixed-arity", "method-params", "arglists",
    "arglists-meta", "export", "type", "num-fields", "record", "meta", "line", "column", "file"];

fn overflow() -> Diagnostic { Diagnostic { span: 0..0, message: "Completed value storage overflow".into() } }
fn storage<T>(count: usize, meter: &mut Meter<'_>) -> Result<(), Diagnostic> {
    let bytes = count.checked_mul(std::mem::size_of::<T>()).and_then(|n| n.checked_mul(2))
        .and_then(|n| count.checked_mul(4).and_then(|c| n.checked_add(c)))
        .and_then(|n| u64::try_from(n).ok()).ok_or_else(overflow)?;
    meter(bytes)
}
fn text(value: &str, meter: &mut Meter<'_>) -> Result<String, Diagnostic> {
    storage::<u8>(value.len(), meter)?;
    storage::<String>(1, meter)?;
    Ok(value.to_owned())
}
fn owned_form(form: &Form, meter: &mut Meter<'_>) -> Result<Arc<Form>, Diagnostic> {
    storage::<Arc<Form>>(1, meter)?;
    meter((std::mem::size_of::<Form>() + 2 * std::mem::size_of::<usize>() + 4) as u64)?;
    Ok(Arc::new(clone_form_metered(form, meter)?))
}
#[derive(Debug)]
pub(crate) struct FunctionValues {
    pub(crate) variadic: bool,
    pub(crate) fixed: Option<usize>,
    pub(crate) parameters: Vec<Vec<String>>,
}
#[derive(Debug)]
pub struct CompletedDeclarationSource {
    global: Global,
    pub(crate) declaration: Form,
    pub(crate) definition: Form,
    pub(crate) origin: Option<SourceOrigin>,
    pub(crate) docstring: Option<Vec<u16>>,
    pub(crate) function: Option<FunctionValues>,
    pub(crate) has_initializer: bool,
    pub(crate) inferred: Option<Form>,
    pub(crate) inferred_return: Option<Form>,
    initializer_identity: Option<std::sync::Weak<hir::Hir>>,
    once: bool,
}
impl CompletedDeclarationSource {
    pub(crate) fn belongs_to(&self, global: &Global) -> bool { self.global == *global }
    pub(crate) fn matches_ordinary(&self, info: &DefinitionInfo, meter: &mut Meter<'_>) -> Result<bool, Diagnostic> {
        meter(40)?;
        if !info.analysis_completed || info.type_fields.is_some() || info.once != self.once
            || info.initializer_form.is_some() != self.has_initializer { return Ok(false); }
        let source = match (&self.origin, &info.origin) {
            (None, None) => true,
            (Some(a), Some(b)) => a.same_native_owner(b),
            _ => false,
        };
        if !source { return Ok(false); }
        if !same_inputs(meter, &self.declaration, &info.declaration)?
            || !same_inputs(meter, &self.definition, &info.definition_form)? { return Ok(false); }
        match (&self.initializer_identity, &info.initializer) {
            (None, None) => (),
            (Some(a), Some(b)) => {
                meter(8)?; let actual = Arc::downgrade(b);
                if !std::sync::Weak::ptr_eq(a, &actual) { return Ok(false); }
            }
            _ => return Ok(false),
        }
        if let Some(initializer) = &info.initializer_form {
            let Kind::List(items) = &self.definition.kind else { return Ok(false); };
            let Some(original) = items.last() else { return Ok(false); };
            if !same_inputs(meter, original, initializer)? { return Ok(false); }
        }
        match (&self.docstring, &info.docstring) {
            (None, None) => Ok(true),
            (Some(a), Some(b)) => native_text(meter, a, b),
            _ => Ok(false),
        }
    }
    pub(crate) fn capture(global: &Global, info: &DefinitionInfo, meter: &mut Meter<'_>)
        -> Result<Option<Arc<Self>>, Diagnostic>
    {
        meter(12)?;
        // Only the genuine completed ordinary native constructor mints this.
        // Provisional and intrinsic type declarations never mint a substitute.
        let ordinary = info.analysis_completed && info.type_fields.is_none()
            && (info.once || matches!(&info.definition_form.kind, Kind::List(items)
                if matches!(items.first().map(|x| &x.kind), Some(Kind::Symbol(s))
                    if s.namespace.is_none() && matches!(s.name.as_str(), "def" | "defonce"))));
        if !ordinary { return Ok(None); }
        storage::<Self>(1, meter)?;
        meter((global.namespace().len() + global.name().len() + 2 * std::mem::size_of::<usize>() + 12) as u64)?;
        let declaration = clone_form_metered(&info.declaration, meter)?;
        let definition = clone_form_metered(&info.definition_form, meter)?;
        let origin = clone_origin(info.origin.as_ref(), meter)?;
        let docstring = if let Some(doc) = &info.docstring {
            storage::<Vec<u16>>(1, meter)?;
            storage::<u16>(doc.len(), meter)?;
            Some(doc.clone())
        } else { None };
        meter(8)?;
        let source = info.initializer.as_ref().and_then(|x| x.source.as_ref());
        let function = if let Some(callable) = source.and_then(|x| x.callable.as_ref()) {
            storage::<FunctionValues>(1, meter)?;
            storage::<Vec<String>>(callable.methods.len(), meter)?;
            let mut parameters = Vec::new();
            parameters.try_reserve_exact(callable.methods.len()).map_err(|_| overflow())?;
            for method in &callable.methods {
                meter(8)?;
                storage::<String>(method.parameters.len(), meter)?;
                let mut names = Vec::new();
                names.try_reserve_exact(method.parameters.len()).map_err(|_| overflow())?;
                for parameter in &method.parameters { names.push(text(&parameter.name, meter)?); }
                parameters.push(names);
            }
            meter((callable.methods.len() * 8 + 8) as u64)?;
            Some(FunctionValues { variadic: callable.variadic(), fixed: callable.max_fixed_arity(), parameters })
        } else { None };
        // Existing native inference only; no metadata/source query or caller
        // callbacks are run here. No initializer owner escapes this operation.
        let inferred = info.initializer.as_ref().map(|x| hir::completed_inferred_value_metered(x, meter)).transpose()?.flatten();
        let inferred_return = source.and_then(|x| x.tags.inferred_return.as_ref()).and_then(|x| x.as_ref())
            .map(|x| clone_form_metered(x, meter)).transpose()?;
        meter((2 * std::mem::size_of::<Option<std::sync::Weak<hir::Hir>>>() + 12) as u64)?;
        let initializer_identity = info.initializer.as_ref().map(Arc::downgrade);
        Ok(Some(Arc::new(Self { global: global.clone(), declaration, definition, origin,
            docstring, function, has_initializer: info.initializer_form.is_some(), inferred, inferred_return,
            initializer_identity, once: info.once })))
    }
}

/// One collapsed overlay, not a chain of older type/definition records. Values
/// are genuine current normalized source values, including explicit nil/false.
#[derive(Debug)]
pub(crate) struct TypeValuePlan {
    pub(crate) source: Option<Arc<CompletedDeclarationSource>>,
    values: [Option<Arc<TypeSourceValue>>; 22],
    known: bool,
    current: Option<TypeValueInput>,
}
#[derive(Debug)]
pub(crate) struct TypeSourceValue {
    form: Form,
    origin: Option<SourceOrigin>,
}
impl TypeSourceValue {
    pub(crate) fn form(&self) -> &Form { &self.form }
    pub(crate) fn origin(&self) -> Option<&SourceOrigin> { self.origin.as_ref() }
    fn capture(form: &Form, origin: Option<&SourceOrigin>, meter: &mut Meter<'_>) -> Result<Arc<Self>, Diagnostic> {
        storage::<Self>(1, meter)?;
        meter((2 * std::mem::size_of::<usize>() + 8) as u64)?;
        let form = clone_form_metered(form, meter)?;
        let origin = clone_origin(origin, meter)?;
        Ok(Arc::new(Self { form, origin }))
    }
    #[cfg(test)]
    pub(crate) fn independent_test_copy(&self) -> Arc<Self> {
        Arc::new(Self { form: self.form.clone(), origin: self.origin.clone() })
    }
}
#[derive(Debug)]
struct TypeValueInput {
    declaration: Arc<Form>,
    definition: Arc<Form>,
    origin: Option<SourceOrigin>,
    fields: Option<usize>,
    record: bool,
}

impl TypeValuePlan {
    pub(crate) fn capture(global: &Global, previous: Option<&DefinitionInfo>, incoming: &Form,
        origin: Option<&SourceOrigin>, meter: &mut Meter<'_>) -> Result<Self, Diagnostic>
    {
        storage::<Self>(1, meter)?;
        meter((22 * 8 + 12) as u64)?;
        let mut source = None;
        let mut known = true;
        let mut values = std::array::from_fn(|_| None);
        if let Some(previous) = previous {
            meter(8)?;
            if previous.type_fields.is_some() {
                if let Some(plan) = previous.type_key_history.as_ref().and_then(|h| h.values(global)) {
                    if plan.matches_current_type(previous, meter)? {
                        meter((22 * 8 + std::mem::size_of::<Self>() + 8) as u64)?;
                        source = plan.source.clone();
                        values = plan.values.clone();
                    } else { known = false; }
                } else { known = false; }
            } else if let Some(owner) = &previous.completed_values {
                meter((global.namespace().len() * 2 + global.name().len() * 2 + 8) as u64)?;
                if owner.belongs_to(global) && owner.matches_ordinary(previous, meter)? {
                    source = Some(owner.clone());
                } else { known = false; }
            } else { known = false; }
        }
        // Each parse-type performs fresh five-default associations, before
        // CURRENT metadata. Other overrides survive later type revisions.
        for name in ["name", "tag", "type", "num-fields", "record"] {
            meter(22 * 4 + 4)?;
            let index = VALUE_FIELDS.iter().position(|n| *n == name).expect("fixed field");
            values[index] = None;
        }
        let entries = hir::reader_metadata_view_metered(incoming, meter)?;
        for entry in entries {
            meter(8)?;
            if let Kind::Keyword(key) = &entry.key.as_ref().kind {
                if key.namespace.is_none() {
                    for (index, label) in VALUE_FIELDS.iter().enumerate() {
                        meter((key.name.len() + label.len() + 4) as u64)?;
                        if key.name == *label {
                            values[index] = Some(TypeSourceValue::capture(entry.value.as_ref(), origin, meter)?);
                            break;
                        }
                    }
                }
            }
        }
        Ok(Self { source, values, known, current: None })
    }
    pub(crate) fn seal(&mut self, info: &DefinitionInfo, meter: &mut Meter<'_>) -> Result<(), Diagnostic> {
        storage::<TypeValueInput>(1, meter)?;
        let declaration = owned_form(&info.declaration, meter)?;
        let definition = owned_form(&info.definition_form, meter)?;
        let origin = clone_origin(info.origin.as_ref(), meter)?;
        meter(8)?;
        self.current = Some(TypeValueInput { declaration, definition, origin,
            fields: info.type_fields, record: info.type_is_record });
        Ok(())
    }
    pub(crate) fn matches_current_type(&self, info: &DefinitionInfo, meter: &mut Meter<'_>) -> Result<bool, Diagnostic> {
        meter(16)?;
        let Some(current) = &self.current else { return Ok(false); };
        if !info.analysis_completed || info.type_fields != current.fields || info.type_is_record != current.record { return Ok(false); }
        let source = match (&current.origin, &info.origin) {
            (None, None) => true,
            (Some(a), Some(b)) => a.same_native_owner(b),
            _ => false,
        };
        Ok(source && same_inputs(meter, &current.declaration, &info.declaration)?
            && same_inputs(meter, &current.definition, &info.definition_form)?)
    }
    pub(crate) fn is_known(&self) -> bool { self.known }
    pub(crate) fn overrides(&self) -> impl Iterator<Item = (&'static str, &Arc<TypeSourceValue>)> {
        VALUE_FIELDS.into_iter().zip(self.values.iter()).filter_map(|(name, x)| x.as_ref().map(|v| (name, v)))
    }
    pub(crate) fn source_value(&self, label: &str) -> Option<&Arc<TypeSourceValue>> {
        VALUE_FIELDS.iter().position(|n| *n == label).and_then(|i| self.values[i].as_ref())
    }
}

fn native_reserve<T>(meter: &mut Meter<'_>, values: &mut Vec<T>, additional: usize) -> Result<(), Diagnostic> {
    meter(8)?;
    let needed = values.len().checked_add(additional).ok_or_else(overflow)?;
    if needed > values.capacity() {
        let capacity = values.capacity().checked_mul(2).unwrap_or(needed).max(needed).max(4);
        let bytes = capacity.checked_add(values.len()).and_then(|x| x.checked_mul(std::mem::size_of::<T>()))
            .and_then(|x| u64::try_from(x).ok()).ok_or_else(overflow)?;
        meter(bytes.checked_add(4).ok_or_else(overflow)?)?;
        values.try_reserve_exact(capacity - values.len()).map_err(|_| overflow())?;
    }
    Ok(())
}
fn native_text<T: PartialEq>(session: &mut Meter<'_>, a: &[T], b: &[T]) -> Result<bool, Diagnostic> {
    session(1)?;
    if a.len() != b.len() { return Ok(false); }
    for (a, b) in a.chunks(256).zip(b.chunks(256)) {
        session((a.len() + b.len()) as u64)?;
        if a != b { return Ok(false); }
    }
    Ok(true)
}
fn namespace(session: &mut Meter<'_>, a: &Option<String>, b: &Option<String>) -> Result<bool, Diagnostic> {
    session(1)?;
    match (a, b) {
        (None, None) => Ok(true),
        (Some(a), Some(b)) => native_text(session, a.as_bytes(), b.as_bytes()),
        _ => Ok(false),
    }
}
/// Strictly stronger than raw-key equality: includes every span and raw metadata
/// prefix, distinguishes kinds and exact numeric bits, and never calls guests.
fn same_inputs(session: &mut Meter<'_>, a: &Form, b: &Form) -> Result<bool, Diagnostic> {
    fn push<'a, 'b>(session: &mut Meter<'_>, pending: &mut Vec<(&'a Form, &'b Form)>,
        a: &'a Form, b: &'b Form) -> Result<(), Diagnostic> {
        native_reserve(session, pending, 1)?;
        session(3)?;
        pending.push((a, b));
        Ok(())
    }
    session((2 * std::mem::size_of::<Vec<(&Form, &Form)>>() + 4) as u64)?;
    let mut pending = Vec::new();
    push(session, &mut pending, a, b)?;
    while let Some((a, b)) = pending.pop() {
        session(8)?;
        if a.span != b.span || a.metadata.len() != b.metadata.len() { return Ok(false); }
        match (&a.kind, &b.kind) {
            (Kind::Nil, Kind::Nil) => {}
            (Kind::Bool(a), Kind::Bool(b)) if a == b => {}
            (Kind::Number(a), Kind::Number(b)) if a.to_bits() == b.to_bits() => {}
            (Kind::String(a), Kind::String(b)) => {
                if !native_text(session, a, b)? { return Ok(false); }
            }
            (Kind::Symbol(a), Kind::Symbol(b)) => {
                if !namespace(session, &a.namespace, &b.namespace)?
                    || !native_text(session, a.name.as_bytes(), b.name.as_bytes())? { return Ok(false); }
            }
            (Kind::Keyword(a), Kind::Keyword(b)) => {
                if !namespace(session, &a.namespace, &b.namespace)?
                    || !native_text(session, a.name.as_bytes(), b.name.as_bytes())? { return Ok(false); }
            }
            (Kind::List(a), Kind::List(b)) | (Kind::Vector(a), Kind::Vector(b))
            | (Kind::Map(a), Kind::Map(b)) | (Kind::Set(a), Kind::Set(b))
            | (Kind::Conditional(a), Kind::Conditional(b))
            | (Kind::SplicingConditional(a), Kind::SplicingConditional(b)) => {
                if a.len() != b.len() { return Ok(false); }
                for (a, b) in a.iter().zip(b).rev() { push(session, &mut pending, a, b)?; }
            }
            (Kind::Discard(a), Kind::Discard(b)) => push(session, &mut pending, a, b)?,
            (Kind::Prefix { operator: ao, target: at }, Kind::Prefix { operator: bo, target: bt }) => {
                push(session, &mut pending, at, bt)?;
                push(session, &mut pending, ao, bo)?;
            }
            _ => return Ok(false),
        }
        for (a, b) in a.metadata.iter().zip(&b.metadata).rev() {
            push(session, &mut pending, a, b)?;
        }
    }
    Ok(true)
}


fn clone_origin(origin: Option<&SourceOrigin>, meter: &mut Meter<'_>) -> Result<Option<SourceOrigin>, Diagnostic> {
    meter((2 * std::mem::size_of::<Option<SourceOrigin>>() + 12) as u64)?;
    if let Some(origin) = origin {
        if let Some(path) = origin.path() {
            let work = path.as_os_str().len().checked_mul(2).and_then(|n| n.checked_add(8))
                .and_then(|n| u64::try_from(n).ok()).ok_or_else(overflow)?;
            meter(work)?;
        }
        Ok(Some(origin.clone()))
    } else { Ok(None) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::portable::resolve::{Environment, Phase};
    #[test]
    fn provisional_and_changed_previous_owners_do_not_mint_known_type_values() {
        let source = "(def ^number subject 17)";
        let forms = suss_reader::forms::read_forms(source).unwrap();
        let origin = SourceOrigin::new(source, None);
        let environment = hir::prepare_with_origin(&forms, 0..source.len(), &Environment::default(), Phase::Macro,
            &mut crate::portable::NoExpansion, Some(&origin)).unwrap().1;
        let binding = environment.resolve(Phase::Macro, &suss_reader::Symbol::new("subject"), 0..0).unwrap();
        let global = binding.global();
        let info = environment.definition_info(global).unwrap();
        let incoming = suss_reader::forms::read_forms("subject").unwrap().remove(0);
        assert!(TypeValuePlan::capture(global, Some(info), &incoming, None, &mut |_| Ok(())).unwrap().is_known());
        let mut provisional = info.clone();
        provisional.analysis_completed = false;
        provisional.completed_values = None;
        assert!(!TypeValuePlan::capture(global, Some(&provisional), &incoming, None, &mut |_| Ok(())).unwrap().is_known());
        let mut changed = info.clone();
        changed.docstring = Some("changed revision".encode_utf16().collect());
        assert!(!TypeValuePlan::capture(global, Some(&changed), &incoming, None, &mut |_| Ok(())).unwrap().is_known());
        // An old opaque owner field retained by a public Info clone must not
        // turn a mutated revision into a genuine completed input.
        let mut changed = info.clone();
        changed.declaration.span.end += 1;
        assert!(!TypeValuePlan::capture(global, Some(&changed), &incoming, None, &mut |_| Ok(())).unwrap().is_known());
    }

    #[test]
    fn completed_source_capture_binds_full_current_inputs_and_retries_every_small_capture_charge() {
        let source = "(def ^number subject 17)";
        let forms = suss_reader::forms::read_forms(source).unwrap();
        let origin = SourceOrigin::new(source, None);
        let environment = hir::prepare_with_origin(&forms, 0..source.len(), &Environment::default(), Phase::Macro,
            &mut crate::portable::NoExpansion, Some(&origin)).unwrap().1;
        let binding = environment.resolve(Phase::Macro, &suss_reader::Symbol::new("subject"), 0..0).unwrap();
        let global = binding.global();
        let info = environment.definition_info(global).unwrap();
        let owner = info.completed_values.as_ref().unwrap();
        assert!(owner.matches_ordinary(info, &mut |_| Ok(())).unwrap());
        let mut changed = info.clone();
        changed.declaration.span.end += 1;
        assert!(!owner.matches_ordinary(&changed, &mut |_| Ok(())).unwrap());
        let mut changed = info.clone();
        changed.origin = Some(SourceOrigin::new(source, None));
        assert!(!owner.matches_ordinary(&changed, &mut |_| Ok(())).unwrap(), "equal text is not original source owner");
        let mut changed = info.clone();
        changed.docstring = Some("different".encode_utf16().collect());
        assert!(!owner.matches_ordinary(&changed, &mut |_| Ok(())).unwrap());
        let mut changed = info.clone();
        changed.initializer = changed.initializer.as_ref().map(|x| Arc::new(x.as_ref().clone()));
        assert!(!owner.matches_ordinary(&changed, &mut |_| Ok(())).unwrap(), "equal new initializer owner is a different revision");
        let mut calls = 0;
        CompletedDeclarationSource::capture(global, info, &mut |_| { calls += 1; Ok(()) }).unwrap();
        for fail in 0..calls {
            let mut seen = 0;
            let error = CompletedDeclarationSource::capture(global, info, &mut |_| {
                let index = seen; seen += 1;
                if index == fail { Err(Diagnostic { span: 0..1, message: "capture-cut".into() }) } else { Ok(()) }
            }).err().expect("reachable capture charge");
            assert_eq!(error.message, "capture-cut");
            let retry = CompletedDeclarationSource::capture(global, info, &mut |_| Ok(())).unwrap().unwrap();
            assert!(retry.matches_ordinary(info, &mut |_| Ok(())).unwrap());
            assert!(owner.matches_ordinary(info, &mut |_| Ok(())).unwrap());
        }
    }
}
