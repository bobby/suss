//! Private reader attachment facts over independently parsed source syntax.
use super::{Form, Kind, ParseError, WorkMeter, charge, error, source_metadata};
use std::{ops::Range, sync::Arc};
const CHUNK: usize = 128;
#[derive(Clone, Copy, Debug)]
struct Site {
    key: usize,
    start: usize,
    end: usize,
}
#[derive(Debug, Default)]
pub(super) struct Capture {
    chunks: Vec<Vec<Site>>,
    last: Option<usize>,
    fallback: bool,
}
fn tag(form: &Form) -> Option<usize> {
    match &form.kind {
        Kind::List(_) => Some(0),
        Kind::Vector(_) => Some(1),
        Kind::Map(_) => Some(2),
        Kind::Set(_) => Some(3),
        Kind::Symbol(s) if s.namespace.is_some() || s.name != "/" => Some(4),
        _ => None,
    }
}
fn key(start: usize, tag: usize) -> Result<usize, ParseError> {
    start
        .checked_mul(8)
        .and_then(|v| v.checked_add(tag))
        .ok_or_else(|| error(start..start, "Source attachment offset overflow"))
}
pub(super) fn reserve<T>(
    v: &mut Vec<T>,
    extra: usize,
    meter: &mut WorkMeter<'_>,
    span: Range<usize>,
) -> Result<(), ParseError> {
    if extra > v.capacity() - v.len() {
        let capacity = v
            .len()
            .saturating_add(extra)
            .max(v.capacity().saturating_mul(2))
            .max(4);
        charge(
            meter,
            (capacity.saturating_add(v.len())).saturating_mul(std::mem::size_of::<T>()) as u64,
            span,
        )?;
        v.reserve_exact(capacity - v.len());
    }
    Ok(())
}
impl Capture {
    pub(super) fn unsupported(&mut self) {
        self.fallback = true;
    }
    pub(super) fn begin(
        &mut self,
        start: usize,
        meter: &mut WorkMeter<'_>,
    ) -> Result<usize, ParseError> {
        charge(meter, 3, start..start)?;
        if self.chunks.last().is_none_or(|c| c.len() == CHUNK) {
            reserve(&mut self.chunks, 1, meter, start..start)?;
            charge(
                meter,
                (CHUNK * std::mem::size_of::<Site>()) as u64,
                start..start,
            )?;
            self.chunks.push(Vec::with_capacity(CHUNK));
        }
        let index = (self.chunks.len() - 1) * CHUNK + self.chunks.last().unwrap().len();
        self.chunks.last_mut().unwrap().push(Site {
            key: usize::MAX,
            start,
            end: start,
        });
        Ok(index)
    }
    fn site_mut(&mut self, id: usize) -> &mut Site {
        &mut self.chunks[id / CHUNK][id % CHUNK]
    }
    pub(super) fn finish(
        &mut self,
        slot: Option<usize>,
        form: &Form,
        meter: &mut WorkMeter<'_>,
    ) -> Result<(), ParseError> {
        charge(meter, 2, form.span.clone())?;
        self.last = if let Some(tag) = tag(form) {
            let id = match slot {
                Some(id) => id,
                None => self.begin(form.span.start, meter)?,
            };
            self.site_mut(id).key = key(form.span.start, tag)?;
            self.site_mut(id).end = form.span.end;
            Some(id)
        } else {
            None
        };
        Ok(())
    }
    pub(super) fn retarget(
        &mut self,
        slot: usize,
        form: &Form,
        meter: &mut WorkMeter<'_>,
    ) -> Result<(), ParseError> {
        charge(meter, 5, form.span.clone())?;
        // Metadata on selection/discard can move a DIFFERENT source node.
        // Keep the existing indexed-reader fallback for this case.
        self.fallback |= matches!(
            &form.kind,
            Kind::Conditional(_) | Kind::SplicingConditional(_) | Kind::Discard(_)
        );
        if let Some(old) = self.last {
            let mut site = *self.site_mut(old);
            self.site_mut(old).key = usize::MAX;
            site.key = key(
                form.span.start,
                tag(form).expect("captured attachment kind"),
            )?;
            site.end = form.span.end;
            *self.site_mut(slot) = site;
            self.last = Some(slot);
        }
        Ok(())
    }
    fn seal(&mut self, meter: &mut WorkMeter<'_>) -> Result<(), ParseError> {
        let mut previous = None;
        for chunk in &mut self.chunks {
            let mut out = 0;
            for i in 0..chunk.len() {
                charge(meter, 3, 0..0)?;
                let site = chunk[i];
                if site.key != usize::MAX {
                    if previous.is_some_and(|p| p >= site.key) {
                        self.fallback = true;
                    }
                    previous = Some(site.key);
                    chunk[out] = site;
                    out += 1;
                }
            }
            chunk.truncate(out);
        }
        let mut out = 0;
        for i in 0..self.chunks.len() {
            charge(meter, 8, 0..0)?;
            if !self.chunks[i].is_empty() {
                self.chunks.swap(out, i);
                out += 1;
            }
        }
        self.chunks.truncate(out);
        self.last = None;
        Ok(())
    }
    fn find(&self, form: &Form, meter: &mut WorkMeter<'_>) -> Result<Option<usize>, ParseError> {
        charge(meter, 2, form.span.clone())?;
        let Some(tag) = tag(form) else {
            return Ok(None);
        };
        let wanted = key(form.span.start, tag)?;
        let (mut lo, mut hi) = (0, self.chunks.len());
        while lo < hi {
            charge(meter, 4, form.span.clone())?;
            let mid = lo + (hi - lo) / 2;
            if self.chunks[mid].last().unwrap().key < wanted {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        let Some(chunk) = self.chunks.get(lo) else {
            return Ok(None);
        };
        let (mut lo, mut hi) = (0, chunk.len());
        while lo < hi {
            charge(meter, 4, form.span.clone())?;
            let mid = lo + (hi - lo) / 2;
            if chunk[mid].key < wanted {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        charge(meter, 2, form.span.clone())?;
        Ok(chunk
            .get(lo)
            .filter(|s| s.key == wanted && s.end == form.span.end)
            .map(|s| s.start))
    }
}

/// Immutable snapshot minted only by the text reader. Node handles can select
/// only this snapshot's actual original nodes, never caller-supplied Forms.
#[derive(Debug)]
pub struct SourceMetadata {
    forms: Vec<Form>,
    sites: Capture,
    positions: Option<source_metadata::Positions>,
    carets: source_metadata::Carets,
    source: Arc<str>,
    file: Option<String>,
}
#[derive(Clone, Copy)]
pub struct SourceMetadataNode<'a> {
    source: &'a SourceMetadata,
    form: &'a Form,
    list_prefix: bool,
}
impl SourceMetadata {
    pub fn read_metered(
        source: Arc<str>,
        file: Option<String>,
        meter: &mut WorkMeter<'_>,
    ) -> Result<Self, ParseError> {
        let (forms, _, mut sites, raw_carets) =
            super::read_forms_internal(&source, None, true, false, meter)?;
        let forms = super::resolve_conditionals_metered(forms, meter)?;
        sites.seal(meter)?;
        let mut carets = source_metadata::Carets::new();
        let positions = if sites.fallback {
            None
        } else {
            charge(meter, source.len() as u64, 0..source.len())?;
            let positions = source_metadata::Positions::new(&source);
            for (span, offset) in raw_carets {
                charge(meter, 40, span.0..span.1)?;
                carets.insert(span, positions.position(&source, offset));
            }
            Some(positions)
        };
        Ok(Self {
            forms,
            sites,
            positions,
            carets,
            source,
            file,
        })
    }
    pub fn forms(&self) -> &[Form] {
        &self.forms
    }
    pub fn requires_eager(&self) -> bool {
        self.sites.fallback
    }
    pub fn root(&self, index: usize) -> Option<SourceMetadataNode<'_>> {
        self.forms.get(index).map(|form| SourceMetadataNode {
            source: self,
            form,
            list_prefix: false,
        })
    }
}
impl<'a> SourceMetadataNode<'a> {
    pub fn form(self) -> &'a Form {
        self.form
    }
    pub fn is_caret_prefix(self) -> bool {
        self.list_prefix
            && self
                .source
                .carets
                .contains_key(&(self.form.span.start, self.form.span.end))
    }
    pub fn child(self, metadata: bool, index: usize) -> Option<Self> {
        let items = if metadata {
            &self.form.metadata
        } else {
            match &self.form.kind {
                Kind::List(v) | Kind::Vector(v) | Kind::Map(v) | Kind::Set(v) => v,
                _ => return None,
            }
        };
        items.get(index).map(|form| Self {
            source: self.source,
            form,
            list_prefix: metadata && matches!(self.form.kind, Kind::List(_)),
        })
    }
    fn attach(
        self,
        form: &mut Form,
        site: Option<usize>,
        meter: &mut WorkMeter<'_>,
    ) -> Result<(), ParseError> {
        if let Some(start) = site {
            let actual = std::mem::replace(&mut form.span, start..self.form.span.end);
            let result = source_metadata::attach_metered(
                &self.source.source,
                self.source
                    .positions
                    .as_ref()
                    .expect("supported reader sites"),
                self.source.file.as_deref(),
                form,
                meter,
            );
            form.span = actual;
            result?;
        }
        Ok(())
    }
    fn clone_attached(self, meter: &mut WorkMeter<'_>) -> Result<Form, ParseError> {
        stacker::maybe_grow(256 * 1024, 4 * 1024 * 1024, || {
            let body = match &self.form.kind {
                Kind::List(v) | Kind::Vector(v) | Kind::Map(v) | Kind::Set(v) => v.len(),
                _ => 0,
            };
            let text = match &self.form.kind {
                Kind::String(v) => v.len().saturating_mul(2),
                Kind::Symbol(v) => v.name.len() + v.namespace.as_ref().map_or(0, String::len),
                Kind::Keyword(v) => v.name.len() + v.namespace.as_ref().map_or(0, String::len),
                _ => 0,
            };
            charge(
                meter,
                (1 + self.form.metadata.len() + body + text) as u64,
                self.form.span.clone(),
            )?;
            let site = self.source.sites.find(self.form, meter)?;
            let mut metadata =
                Vec::with_capacity(self.form.metadata.len() + usize::from(site.is_some()));
            for i in 0..self.form.metadata.len() {
                metadata.push(self.child(true, i).unwrap().clone_attached(meter)?);
            }
            let mut items = Vec::with_capacity(body);
            for i in 0..body {
                items.push(self.child(false, i).unwrap().clone_attached(meter)?);
            }
            let kind = match &self.form.kind {
                Kind::List(_) => Kind::List(items),
                Kind::Vector(_) => Kind::Vector(items),
                Kind::Map(_) => Kind::Map(items),
                Kind::Set(_) => Kind::Set(items),
                other => other.clone(),
            };
            let mut form = Form {
                span: self.form.span.clone(),
                metadata,
                kind,
            };
            self.attach(&mut form, site, meter)?;
            Ok(form)
        })
    }
    pub fn located_metered(self, meter: &mut WorkMeter<'_>) -> Result<Form, ParseError> {
        if self.source.requires_eager() {
            return Err(error(
                self.form.span.clone(),
                "Source attachment mapping requires indexed-reader fallback",
            ));
        }
        let mut form = self.clone_attached(meter)?;
        if self.is_caret_prefix() {
            let mut carrier = Form {
                span: form.span.clone(),
                metadata: vec![form],
                kind: Kind::List(vec![]),
            };
            source_metadata::list_prefixes_metered(
                std::slice::from_mut(&mut carrier),
                &self.source.carets,
                meter,
            )?;
            form = carrier.metadata.remove(0);
        } else {
            source_metadata::list_prefixes_metered(
                std::slice::from_mut(&mut form),
                &self.source.carets,
                meter,
            )?;
        }
        Ok(form)
    }
    pub fn root_metadata_metered(
        self,
        meter: &mut WorkMeter<'_>,
    ) -> Result<(Range<usize>, Vec<Form>), ParseError> {
        if self.source.requires_eager() {
            return Err(error(
                self.form.span.clone(),
                "Source attachment mapping requires indexed-reader fallback",
            ));
        }
        let site = self.source.sites.find(self.form, meter)?;
        let capacity = self.form.metadata.len() + usize::from(site.is_some());
        charge(
            meter,
            (capacity * std::mem::size_of::<Form>()) as u64,
            self.form.span.clone(),
        )?;
        let mut metadata = Vec::with_capacity(capacity);
        for i in 0..self.form.metadata.len() {
            metadata.push(self.child(true, i).unwrap().clone_attached(meter)?);
        }
        // Only the List discriminant affects list-prefix caret normalization.
        let kind = if matches!(self.form.kind, Kind::List(_)) {
            Kind::List(vec![])
        } else {
            Kind::Nil
        };
        let mut carrier = Form {
            span: self.form.span.clone(),
            metadata,
            kind,
        };
        // attach_metered needs the actual root kind; a body-free carrier keeps
        // that discriminant without copying the definition payload.
        if let Some(start) = site {
            let mut stub = Form {
                span: start..self.form.span.end,
                metadata: vec![],
                kind: Kind::List(vec![]),
            };
            source_metadata::attach_metered(
                &self.source.source,
                self.source
                    .positions
                    .as_ref()
                    .expect("supported reader sites"),
                self.source.file.as_deref(),
                &mut stub,
                meter,
            )?;
            carrier.metadata.extend(std::mem::take(&mut stub.metadata));
        }
        source_metadata::list_prefixes_metered(
            std::slice::from_mut(&mut carrier),
            &self.source.carets,
            meter,
        )?;
        Ok((carrier.span.clone(), std::mem::take(&mut carrier.metadata)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn demand_snapshot_matches_indexed_reader_on_complete_sources_and_root_projection() {
        for source in [
            "(def ^:dynamic target [foo {:a #{1 2}}])",
            "^{:line 99 :custom true} ^:outer (f ^number arg)\r\n[é 😀tag \\\\uD800]",
            "'x @y `(~x ~@ys) [^:v value ^{:column 88} other]",
            "^:quoted '(unresolved x) ^{:file \"override\"} @value",
            "#?(:cljs ^:selected [foo] :clj [wrong]) #_discarded #{kept}",
            "^:a ^:b (f) ^{:line 4 :column 5 :end-line 6 :end-column 7} value /",
        ] {
            let source = if source.contains("\\uD800") {
                source.replace("\\\\uD800", "\"\\uD800\"")
            } else {
                source.to_owned()
            };
            let snapshot = SourceMetadata::read_metered(
                Arc::from(source.as_str()),
                Some("path/😀.sus".into()),
                &mut |_| Ok(()),
            )
            .unwrap();
            assert!(!snapshot.requires_eager(), "{source}");
            let indexed =
                super::super::read_forms_with_source_metadata(&source, Some("path/😀.sus"))
                    .unwrap();
            fn verify(node: SourceMetadataNode<'_>, expected: &Form, source: &str) {
                assert_eq!(
                    &node.located_metered(&mut |_| Ok(())).unwrap(),
                    expected,
                    "{source}"
                );
                let (span, meta) = node.root_metadata_metered(&mut |_| Ok(())).unwrap();
                assert_eq!(span, expected.span, "{source}");
                assert_eq!(meta, expected.metadata, "{source}");
                for i in 0..node.form().metadata.len() {
                    verify(node.child(true, i).unwrap(), &expected.metadata[i], source);
                }
                if let (
                    Kind::List(a) | Kind::Vector(a) | Kind::Map(a) | Kind::Set(a),
                    Kind::List(b) | Kind::Vector(b) | Kind::Map(b) | Kind::Set(b),
                ) = (&node.form().kind, &expected.kind)
                {
                    // List-caret normalization prepends line/column pairs to
                    // the original metadata map's body; original entries move.
                    let offset = if node.is_caret_prefix() { 4 } else { 0 };
                    for i in 0..a.len() {
                        verify(node.child(false, i).unwrap(), &b[offset + i], source);
                    }
                }
            }
            for (i, expected) in indexed.iter().enumerate() {
                verify(snapshot.root(i).unwrap(), expected, &source);
            }
        }
    }
    #[test]
    fn snapshot_cancellation_does_not_publish_partial_reader_facts() {
        let source: Arc<str> = Arc::from("^:x (f [a b c] {:k 17})");
        let mut total = 0;
        SourceMetadata::read_metered(source.clone(), None, &mut |n| {
            total += n;
            Ok(())
        })
        .unwrap();
        for limit in [0, 1, 16, total / 2, total - 1] {
            let mut work = 0;
            assert!(
                SourceMetadata::read_metered(source.clone(), None, &mut |n| {
                    work += n;
                    if work > limit {
                        Err(error(0..1, "cancelled"))
                    } else {
                        Ok(())
                    }
                })
                .is_err()
            );
            let recovered =
                SourceMetadata::read_metered(source.clone(), None, &mut |_| Ok(())).unwrap();
            let node = recovered.root(0).unwrap();
            assert!(
                node.located_metered(&mut |_| Err(error(0..1, "cancelled")))
                    .is_err()
            );
            assert_eq!(
                node.located_metered(&mut |_| Ok(())).unwrap(),
                super::super::read_forms_with_source_metadata(&source, None)
                    .unwrap()
                    .remove(0)
            );
        }
    }
    #[test]
    fn unsupported_selection_metadata_and_anonymous_forms_require_explicit_fallback() {
        for source in ["^:x #?(:cljs [kept])", "#(+ % 17)"] {
            let snapshot =
                SourceMetadata::read_metered(Arc::from(source), None, &mut |_| Ok(())).unwrap();
            assert!(snapshot.requires_eager());
            assert!(
                snapshot
                    .root(0)
                    .unwrap()
                    .located_metered(&mut |_| Ok(()))
                    .is_err()
            );
        }
    }
}
