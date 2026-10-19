//! Collapsed first-key origins for the remaining existing declaration fields.
//! Field policy follows the existing graph's adaptation of ClojureScript
//! analyzer.cljc parse :def/source-info and parse-type, pin
//! c4295f303100bbf5afac449242d30bca1126f1a1 (upstream EPL-1.0).
//! Upstream source SHA-256: 297802c627474434f1ef868e31f5f9913c290a4e80c509a40c704dced95bbf47.
//! Native fact storage, metering and this key-only projection are original code.
use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RetainedKey {
    Ns, Private, Dynamic, Doc, Declared, RetTag, FnVar, Variadic, MaxFixedArity,
    MethodParams, Arglists, ArglistsMeta, Export, Meta, Line, Column, File,
}
impl RetainedKey {
    pub(crate) const ALL: [Self; 17] = [Self::Ns, Self::Private, Self::Dynamic, Self::Doc,
        Self::Declared, Self::RetTag, Self::FnVar, Self::Variadic, Self::MaxFixedArity,
        Self::MethodParams, Self::Arglists, Self::ArglistsMeta, Self::Export, Self::Meta,
        Self::Line, Self::Column, Self::File];
    pub(crate) const FUNCTION: [Self; 6] = [Self::FnVar, Self::Variadic, Self::MaxFixedArity,
        Self::MethodParams, Self::Arglists, Self::ArglistsMeta];
    pub(crate) fn name(self) -> &'static str {
        match self { Self::Ns => "ns", Self::Private => "private", Self::Dynamic => "dynamic",
            Self::Doc => "doc", Self::Declared => "declared", Self::RetTag => "ret-tag",
            Self::FnVar => "fn-var", Self::Variadic => "variadic?", Self::MaxFixedArity => "max-fixed-arity",
            Self::MethodParams => "method-params", Self::Arglists => "arglists", Self::ArglistsMeta => "arglists-meta",
            Self::Export => "export", Self::Meta => "meta", Self::Line => "line", Self::Column => "column", Self::File => "file" }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RetainedSite {
    CompletedMeta, Docstring, SourceInfo(RetainedKey), CoreFile,
    FunctionField(RetainedKey), ForwardDeclared(RetainedKey),
}
#[derive(Debug, Clone)]
pub(super) struct RetainedHistory {
    pub(super) keys: [Option<KeyOrigin>; 17],
}
impl RetainedHistory {
    pub(super) fn empty() -> Self { Self { keys: std::array::from_fn(|_| None) } }
    pub(super) fn unknown() -> Self { Self { keys: std::array::from_fn(|_| Some(KeyOrigin::Unknown)) } }
    fn literal(&mut self, key: RetainedKey, site: RetainedSite, meter: &mut Meter<'_>) -> Result<(), Diagnostic> {
        meter(4)?;
        if self.keys[key as usize].is_none() {
            self.keys[key as usize] = Some(KeyOrigin::Literal(LiteralSite::Retained(site)));
        }
        Ok(())
    }
    fn source(&mut self, field: RetainedKey, key: &Form, meter: &mut Meter<'_>) -> Result<(), Diagnostic> {
        meter(4)?;
        if self.keys[field as usize].is_none() {
            self.keys[field as usize] = Some(TypeKeyHistory::source(key, false, meter)?);
        }
        Ok(())
    }
    pub(super) fn merge_metadata(&mut self, form: &Form, meter: &mut Meter<'_>) -> Result<(), Diagnostic> {
        let entries = hir::reader_metadata_view_metered(form, meter)?;
        for field in RetainedKey::ALL {
            meter(2)?;
            if let Some(entry) = TypeKeyHistory::property(&entries, field.name(), meter)? {
                self.source(field, entry.key.as_ref(), meter)?;
            }
        }
        Ok(())
    }
    pub(super) fn source_info(&mut self, info: &DefinitionInfo, global: &Global, meter: &mut Meter<'_>) -> Result<(), Diagnostic> {
        meter(6)?;
        if let Some(origin) = &info.origin {
            // A proof snapshot must not force a newly observable non-Unicode
            // path error during otherwise valid type analysis. Classify the
            // actual immutable path before querying; never catch/erase source,
            // allocation, fuel or interrupt failures from the ordinary query.
            meter(2)?;
            let unsupported_path = if let Some(path) = origin.path() {
                meter((path.as_os_str().len() + 2) as u64)?;
                path.to_str().is_none()
            } else { false };
            if unsupported_path {
                for field in [RetainedKey::Line, RetainedKey::Column, RetainedKey::File] {
                    meter(4)?;
                    if self.keys[field as usize].is_none() { self.keys[field as usize] = Some(KeyOrigin::Unknown); }
                }
            } else {
                let (span, metadata) = origin.macro_form_root_metadata_metered(&info.definition_form, &mut |work| meter(work))?;
                meter((std::mem::size_of::<Form>() + 4) as u64)?;
                let carrier = Form { span, metadata, kind: Kind::Nil };
                let entries = hir::reader_metadata_view_metered(&carrier, meter)?;
                for field in [RetainedKey::Line, RetainedKey::Column, RetainedKey::File] {
                    if TypeKeyHistory::property(&entries, field.name(), meter)?.is_some() {
                        self.literal(field, RetainedSite::SourceInfo(field), meter)?;
                    }
                }
            }
        }
        meter((global.namespace().len() + "suss.core".len() + 2) as u64)?;
        if global.namespace() == "suss.core" { self.literal(RetainedKey::File, RetainedSite::CoreFile, meter)?; }
        Ok(())
    }
    pub(super) fn ordinary(global: &Global, info: &DefinitionInfo, meter: &mut Meter<'_>) -> Result<Self, Diagnostic> {
        meter((std::mem::size_of::<Self>() * 2 + 16) as u64)?;
        let ordinary = matches!(&info.definition_form.kind, Kind::List(items)
            if matches!(items.first().map(|head| &head.kind), Some(Kind::Symbol(head))
                if head.namespace.is_none() && matches!(head.name.as_str(), "def" | "defonce"))
            && matches!(items.get(1).map(|name| &name.kind), Some(Kind::Symbol(_))));
        if !ordinary { return Ok(Self::unknown()); }
        let mut out = Self::empty();
        out.merge_metadata(&info.declaration, meter)?;
        let metadata = hir::reader_metadata_view_metered(&info.declaration, meter)?;
        let truthy = |form: &Form| !matches!(form.kind, Kind::Nil | Kind::Bool(false));
        let declared = TypeKeyHistory::property(&metadata, "declared", meter)?.is_some_and(|entry| truthy(entry.value.as_ref()));
        meter(8)?;
        let provisional = !info.analysis_completed || (info.initializer_form.is_some() && declared);
        if !provisional { out.literal(RetainedKey::Meta, RetainedSite::CompletedMeta, meter)?; }
        out.source_info(info, global, meter)?;
        if !provisional && info.docstring.is_some() { out.literal(RetainedKey::Doc, RetainedSite::Docstring, meter)?; }
        let source = info.initializer.as_ref().and_then(|init| init.source.as_ref());
        let callable = source.and_then(|source| source.callable.as_ref());
        if callable.is_some() && !declared {
            if let Some(top) = TypeKeyHistory::property(&metadata, "top-fn", meter)?.map(|entry| entry.value.as_ref())
                .filter(|form| !matches!(form.kind, Kind::Nil))
            {
                // This literal is associated BEFORE the top-fn data is merged.
                out.literal(RetainedKey::FnVar, RetainedSite::FunctionField(RetainedKey::FnVar), meter)?;
                let items = match &top.kind {
                    Kind::Map(items) => Some(items.as_slice()),
                    Kind::Vector(items) if items.len() == 2 => Some(items.as_slice()),
                    Kind::List(items) | Kind::Set(items) if items.is_empty() => Some(&[][..]),
                    Kind::String(units) if units.is_empty() => Some(&[][..]),
                    _ => None,
                };
                for field in RetainedKey::ALL {
                    // Export is not copied by the existing top-fn projection.
                    meter(3)?;
                    if field == RetainedKey::Export { continue; }
                    if let Some(items) = items {
                        for pair in items.chunks_exact(2) {
                            meter((field.name().len() + 6) as u64)?;
                            if let Kind::Keyword(key) = &pair[0].kind {
                                meter((key.name.len() + 2) as u64)?;
                                if key.namespace.is_none() && key.name == field.name() {
                                    out.source(field, &pair[0], meter)?; break;
                                }
                            }
                        }
                    } else {
                        meter(2)?;
                        if out.keys[field as usize].is_none() { out.keys[field as usize] = Some(KeyOrigin::Unknown); }
                    }
                }
            } else {
                for field in RetainedKey::FUNCTION { out.literal(field, RetainedSite::FunctionField(field), meter)?; }
            }
        }
        if !provisional && declared
            && TypeKeyHistory::property(&metadata, "arglists", meter)?.is_some_and(|entry| truthy(entry.value.as_ref()))
        {
            for field in [RetainedKey::Declared, RetainedKey::FnVar, RetainedKey::MethodParams] {
                out.literal(field, RetainedSite::ForwardDeclared(field), meter)?;
            }
        }
        if !provisional && callable.is_some() {
            let explicit = TypeKeyHistory::property(&metadata, "tag", meter)?.is_some_and(|entry| truthy(entry.value.as_ref()));
            meter(4)?;
            if explicit || source.is_some_and(|source| source.tags.inferred_return.as_ref().is_some_and(Option::is_some)) {
                // The completed return-tag association is separate from :tag.
                out.literal(RetainedKey::RetTag, RetainedSite::FunctionField(RetainedKey::RetTag), meter)?;
            }
        }
        Ok(out)
    }
}
