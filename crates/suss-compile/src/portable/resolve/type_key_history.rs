//! Native first-key facts for the five explicit parse-type associations.
//! No predecessor DefinitionInfo, initializer, namespace or guest value is retained.
use super::{DefinitionInfo, Global};
use crate::portable::{Diagnostic, clone_form_metered, hir};
use suss_reader::forms::{Form, Kind};
use std::sync::Arc;

type Meter<'a> = dyn FnMut(u64) -> Result<(), Diagnostic> + 'a;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TypeKey {
    Name, Tag, Type, NumFields, Record,
}
impl TypeKey {
    pub(crate) const ALL: [Self; 5] = [Self::Name, Self::Tag, Self::Type, Self::NumFields, Self::Record];
    pub(crate) fn name(self) -> &'static str {
        match self { Self::Name => "name", Self::Tag => "tag", Self::Type => "type",
            Self::NumFields => "num-fields", Self::Record => "record" }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LiteralSite {
    OrdinaryName,
    CompletedTag,
    ParseType(TypeKey),
}
#[derive(Debug, Clone)]
pub(crate) enum KeyOrigin {
    Literal(LiteralSite),
    Source(Arc<Form>),
    ReaderShorthandTag(Arc<Form>),
    Unknown,
}
/// Opaque, immutable native provenance. Actual Global includes caller phase.
/// Exactly five key facts; redefinitions copy these facts, never a history chain.
#[derive(Debug)]
pub struct TypeKeyHistory {
    global: Global,
    keys: [KeyOrigin; 5],
}
impl TypeKeyHistory {
    pub(crate) fn key(&self, global: &Global, field: TypeKey) -> Option<&KeyOrigin> {
        (self.global == *global).then(|| &self.keys[field as usize])
    }
    pub(crate) fn capture(global: &Global, previous: Option<&DefinitionInfo>, meter: &mut Meter<'_>)
        -> Result<Arc<Self>, Diagnostic>
    {
        // Header, all five controls/entries, Arc ownership/retirement and cloned
        // Global backing are prepaid. Source Form payloads are separately metered.
        meter((std::mem::size_of::<Self>() + 4 * std::mem::size_of::<usize>()
            + global.namespace().len() + global.name().len() + 32) as u64)?;
        let mut keys = TypeKey::ALL.map(|field| KeyOrigin::Literal(LiteralSite::ParseType(field)));
        if let Some(previous) = previous {
            meter(8)?;
            if let Some(history) = &previous.type_key_history {
                // A foreign native snapshot cannot authorize this binding.
                meter((history.global.namespace().len() + history.global.name().len() + 5) as u64)?;
                keys = if history.global == *global { history.keys.clone() }
                    else { std::array::from_fn(|_| KeyOrigin::Unknown) };
            } else if previous.type_fields.is_some() {
                // An older/unproved type producer is not a fresh parse-type site.
                keys = std::array::from_fn(|_| KeyOrigin::Unknown);
            } else {
                Self::ordinary_keys(previous, &mut keys, meter)?;
            }
        }
        Ok(Arc::new(Self { global: global.clone(), keys }))
    }
    fn source(key: &Form, shorthand: bool, meter: &mut Meter<'_>) -> Result<KeyOrigin, Diagnostic> {
        meter((std::mem::size_of::<Form>() + 4 * std::mem::size_of::<usize>() + 4) as u64)?;
        let key = Arc::new(clone_form_metered(key, meter)?);
        Ok(if shorthand { KeyOrigin::ReaderShorthandTag(key) } else { KeyOrigin::Source(key) })
    }
    fn property<'a, 'b>(metadata: &'a [hir::MetadataEntry<'b>], name: &str, meter: &mut Meter<'_>)
        -> Result<Option<&'a hir::MetadataEntry<'b>>, Diagnostic>
    {
        for entry in metadata {
            meter((name.len() + 6) as u64)?;
            if let Kind::Keyword(key) = &entry.key.as_ref().kind {
                meter((key.name.len() + key.namespace.as_ref().map_or(0, String::len) + 2) as u64)?;
                if key.namespace.is_none() && key.name == name { return Ok(Some(entry)); }
            }
        }
        Ok(None)
    }
    fn ordinary_keys(info: &DefinitionInfo, keys: &mut [KeyOrigin; 5], meter: &mut Meter<'_>)
        -> Result<(), Diagnostic>
    {
        meter(12)?;
        let ordinary = matches!(&info.definition_form.kind, Kind::List(items)
            if matches!(items.first().map(|head| &head.kind), Some(Kind::Symbol(head))
                if head.namespace.is_none() && matches!(head.name.as_str(), "def" | "defonce"))
            && matches!(items.get(1).map(|name| &name.kind), Some(Kind::Symbol(_))));
        if !ordinary {
            *keys = std::array::from_fn(|_| KeyOrigin::Unknown);
            return Ok(());
        }
        keys[TypeKey::Name as usize] = KeyOrigin::Literal(LiteralSite::OrdinaryName);
        let metadata = hir::reader_metadata_view_metered(&info.declaration, meter)?;
        for field in [TypeKey::Tag, TypeKey::Type, TypeKey::NumFields, TypeKey::Record] {
            meter(2)?;
            if let Some(entry) = Self::property(&metadata, field.name(), meter)? {
                keys[field as usize] = Self::source(entry.key.as_ref(),
                    entry.key_origin() == hir::MetadataKeyOrigin::ReaderShorthandTag, meter)?;
            }
        }
        meter(8)?;
        let raw_tag = Self::property(&metadata, "tag", meter)?;
        let truthy = |form: &Form| !matches!(form.kind, Kind::Nil | Kind::Bool(false));
        let declared = Self::property(&metadata, "declared", meter)?.is_some_and(|entry| truthy(entry.value.as_ref()));
        let provisional = !info.analysis_completed || (info.initializer_form.is_some() && declared);
        let callable = info.initializer.as_ref().and_then(|init| init.source.as_ref())
            .and_then(|source| source.callable.as_ref());
        // A callable top-fn can add an absent raw :tag key. Existing metadata
        // wins the KEY independently of this later value writer.
        if !provisional && callable.is_some() && !declared && raw_tag.is_none() {
            if let Some(top) = Self::property(&metadata, "top-fn", meter)?.map(|entry| entry.value.as_ref())
                .filter(|form| !matches!(form.kind, Kind::Nil))
            {
                let tag = match &top.kind {
                    Kind::Map(items) => {
                        let mut selected = None;
                        for pair in items.chunks_exact(2) {
                            meter(8)?;
                            if matches!(&pair[0].kind, Kind::Keyword(key)
                                if key.namespace.is_none() && key.name == "tag") { selected = Some(&pair[0]); break; }
                        }
                        selected
                    }
                    Kind::Vector(items) if items.len() == 2 => {
                        meter(8)?;
                        matches!(&items[0].kind, Kind::Keyword(key)
                            if key.namespace.is_none() && key.name == "tag").then_some(&items[0])
                    }
                    Kind::List(items) | Kind::Set(items) if items.is_empty() => None,
                    Kind::String(units) if units.is_empty() => None,
                    // Keep invalid/unproved prior recipes unknown. Do not add a
                    // new semantic error before the existing transport path.
                    _ => { keys[TypeKey::Tag as usize] = KeyOrigin::Unknown; None }
                };
                if let Some(tag) = tag { keys[TypeKey::Tag as usize] = Self::source(tag, false, meter)?; }
            }
        } else if !provisional && callable.is_none() && raw_tag.is_none()
            && hir::declaration_tag_metered(info, meter)?.is_some()
        {
            keys[TypeKey::Tag as usize] = KeyOrigin::Literal(LiteralSite::CompletedTag);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::portable::{self, resolve::{Environment, Phase}};
    fn analyze(source: &str, environment: &Environment, phase: Phase) -> Environment {
        let forms = suss_reader::forms::read_forms(source).unwrap();
        portable::prepare(&forms, 0..source.len(), environment, phase).unwrap().1
    }
    fn subject(environment: &Environment, phase: Phase) -> (Global, &DefinitionInfo) {
        let binding = environment.resolve(phase, &suss_reader::Symbol::new("subject"), 0..0).unwrap();
        let global = binding.global().clone();
        let info = environment.definition_info(&global).unwrap();
        (global, info)
    }
    #[test]
    fn type_key_history_captures_fresh_defaults_before_current_raw_overrides() {
        for phase in [Phase::Runtime, Phase::Macro] {
            for op in ["deftype*", "defrecord*"] {
                let source = format!("({op} ^{{:tag raw-tag :type false :num-fields 99 :record false}} subject [] nil nil)");
                let environment = analyze(&source, &Environment::default(), phase);
                let (global, info) = subject(&environment, phase);
                let history = info.type_key_history.as_ref().unwrap();
                for field in TypeKey::ALL {
                    assert!(matches!(history.key(&global, field), Some(KeyOrigin::Literal(LiteralSite::ParseType(found))) if *found == field));
                }
                assert_eq!(info.type_fields, Some(0));
                assert_eq!(info.type_is_record, op == "defrecord*");
                let pairs = hir::reader_metadata_pairs(&info.declaration).unwrap();
                assert!(pairs.chunks_exact(2).any(|pair| matches!(&pair[1].kind, Kind::Symbol(name) if name.name == "raw-tag")), "values stay on the original path");
            }
        }
    }
    #[test]
    fn type_key_history_preserves_original_raw_keys_through_repeated_types() {
        for phase in [Phase::Runtime, Phase::Macro] {
            let source = "(def ^{:tag raw-tag :type raw-type :num-fields raw-count :record raw-record} subject 17)";
            let mut environment = analyze(source, &Environment::default(), phase);
            let (_, original) = subject(&environment, phase);
            let original_declaration = original.declaration.clone();
            let original_environment = environment.clone();
            let original_pairs = hir::reader_metadata_pairs(&original.declaration).unwrap();
            environment = analyze("(defrecord* subject [] nil nil)", &environment, phase);
            let (global, info) = subject(&environment, phase);
            let first = info.type_key_history.as_ref().unwrap().clone();
            assert!(matches!(first.key(&global, TypeKey::Name), Some(KeyOrigin::Literal(LiteralSite::OrdinaryName))));
            for field in [TypeKey::Tag, TypeKey::Type, TypeKey::NumFields, TypeKey::Record] {
                let Some(KeyOrigin::Source(key)) = first.key(&global, field) else { panic!("missing source key"); };
                let expected = original_pairs.chunks_exact(2).find(|pair| matches!(&pair[0].kind, Kind::Keyword(key) if key.name == field.name())).unwrap();
                assert_eq!(key.as_ref(), &expected[0]);
            }
            for _ in 0..8 {
                environment = analyze("(deftype* ^{:tag replacement :record true} subject [x] nil nil)", &environment, phase);
                let (global, info) = subject(&environment, phase);
                let next = info.type_key_history.as_ref().unwrap();
                assert_eq!(next.keys.len(), 5);
                assert_eq!(info.type_fields, Some(1));
                assert!(!info.type_is_record);
                for field in [TypeKey::Tag, TypeKey::Type, TypeKey::NumFields, TypeKey::Record] {
                    let (Some(KeyOrigin::Source(old)), Some(KeyOrigin::Source(new))) = (first.key(&global, field), next.key(&global, field)) else { panic!("lost key"); };
                    assert!(Arc::ptr_eq(old, new), "native owner retained, not equal-content reconstructed");
                }
            }
            assert_eq!(subject(&original_environment, phase).1.declaration, original_declaration);
        }
    }
    #[test]
    fn type_key_history_distinguishes_completed_tag_and_top_fn_source_keys() {
        for phase in [Phase::Runtime, Phase::Macro] {
            let environment = analyze("(def subject 17)", &Environment::default(), phase);
            let (global, info) = subject(&environment, phase);
            let numeric = TypeKeyHistory::capture(&global, Some(info), &mut |_| Ok(())).unwrap();
            assert!(matches!(numeric.key(&global, TypeKey::Tag), Some(KeyOrigin::Literal(LiteralSite::CompletedTag))));
            let environment = analyze("(def ^{:top-fn {:tag custom-tag}} subject (fn* [] 17))", &Environment::default(), phase);
            let (global, info) = subject(&environment, phase);
            let function = TypeKeyHistory::capture(&global, Some(info), &mut |_| Ok(())).unwrap();
            assert!(matches!(function.key(&global, TypeKey::Tag), Some(KeyOrigin::Source(key))
                if matches!(&key.kind, Kind::Keyword(key) if key.name == "tag")));
            let mut unknown = info.clone();
            unknown.type_fields = Some(0);
            let history = TypeKeyHistory::capture(&global, Some(&unknown), &mut |_| Ok(())).unwrap();
            assert!(TypeKey::ALL.into_iter().all(|field| matches!(history.key(&global, field), Some(KeyOrigin::Unknown))));
        }
    }
    #[test]
    fn type_key_history_rejects_foreign_binding_and_propagates_every_meter_error() {
        let environment = analyze("(def ^number subject 17)", &Environment::default(), Phase::Runtime);
        let (global, info) = subject(&environment, Phase::Runtime);
        let original = info.declaration.clone();
        let mut calls = 0;
        let history = TypeKeyHistory::capture(&global, Some(info), &mut |_| { calls += 1; Ok(()) }).unwrap();
        assert!(matches!(history.key(&global, TypeKey::Tag), Some(KeyOrigin::ReaderShorthandTag(_))));
        for stop in 0..calls {
            let mut current = 0;
            let error = TypeKeyHistory::capture(&global, Some(info), &mut |_| {
                let fail = current == stop; current += 1;
                if fail { Err(Diagnostic { span: 81..85, message: "history meter error".into() }) } else { Ok(()) }
            }).unwrap_err();
            assert_eq!(error.span, 81..85);
            assert_eq!(error.message, "history meter error");
            assert_eq!(info.declaration, original);
            TypeKeyHistory::capture(&global, Some(info), &mut |_| Ok(())).unwrap();
        }
        let mut prior_type = info.clone();
        prior_type.type_key_history = Some(history.clone());
        for foreign in [Global { phase: Phase::Macro, ..global.clone() },
            Global { namespace: "elsewhere".into(), ..global.clone() },
            Global { name: "other".into(), ..global.clone() }]
        {
            assert!(history.key(&foreign, TypeKey::Tag).is_none());
            let copied = TypeKeyHistory::capture(&foreign, Some(&prior_type), &mut |_| Ok(())).unwrap();
            assert!(TypeKey::ALL.into_iter().all(|field| matches!(copied.key(&foreign, field), Some(KeyOrigin::Unknown))));
        }
    }
}
