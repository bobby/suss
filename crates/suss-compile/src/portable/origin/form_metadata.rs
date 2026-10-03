//! Original provenance matching for opt-in indexing-reader macro data.
use super::SourceOrigin;
use crate::portable::Diagnostic;
use suss_reader::forms::{
    Form, Kind, read_forms, read_forms_with_source_metadata, resolve_conditionals,
};

fn code_children(form: &Form) -> &[Form] {
    match &form.kind {
        Kind::List(items) | Kind::Vector(items) | Kind::Map(items) | Kind::Set(items) => {
            items.as_slice()
        }
        _ => &[],
    }
}
fn children(form: &Form) -> impl Iterator<Item = &Form> {
    form.metadata.iter().chain(code_children(form))
}

fn same(a: &Form, b: &Form, metadata: bool) -> bool {
    if a.span != b.span {
        return false;
    }
    if metadata
        && (a.metadata.len() != b.metadata.len()
            || !a
                .metadata
                .iter()
                .zip(&b.metadata)
                .all(|(a, b)| same(a, b, true)))
    {
        return false;
    }
    match (&a.kind, &b.kind) {
        (Kind::Number(a), Kind::Number(b)) => a.to_bits() == b.to_bits(),
        (Kind::List(a), Kind::List(b))
        | (Kind::Vector(a), Kind::Vector(b))
        | (Kind::Map(a), Kind::Map(b))
        | (Kind::Set(a), Kind::Set(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| same(a, b, metadata))
        }
        (a, b) => a == b,
    }
}

fn find<'a>(
    forms: &'a [Form],
    wanted: &Form,
    metadata: bool,
    budget: &mut usize,
) -> Result<Option<&'a Form>, Diagnostic> {
    let mut pending: Vec<&Form> = forms.iter().rev().collect();
    while let Some(form) = pending.pop() {
        *budget = budget.checked_sub(1).ok_or_else(|| Diagnostic {
            span: wanted.span.clone(),
            message: "Source metadata provenance exceeds traversal bound".into(),
        })?;
        if same(form, wanted, metadata) {
            return Ok(Some(form));
        }
        pending.extend(children(form));
    }
    Ok(None)
}

impl SourceOrigin {
    /// Reader metadata for a source form whose entire original syntax and spans
    /// still match. Expanded data with reused spans retains only explicit meta.
    /// Neither provenance construction nor comparison evaluates source.
    pub fn macro_form_data(&self, form: &Form) -> Result<Form, Diagnostic> {
        let mut budget: usize = 1_048_576;
        if let Some(pair) = &self.reader_forms {
            let mut pending = vec![(&pair.0, &pair.1)];
            while let Some((lowered, reader)) = pending.pop() {
                budget = budget.checked_sub(1).ok_or_else(|| Diagnostic {
                    span: form.span.clone(),
                    message: "Reader source provenance exceeds traversal bound".into(),
                })?;
                if same(lowered, form, true) {
                    return Ok(reader.clone());
                }
                // Metadata has an independent shape; source indexing may add
                // prefixes. Never let it shift correspondence of code children.
                pending.extend(lowered.metadata.iter().zip(&reader.metadata));
                if std::mem::discriminant(&lowered.kind) == std::mem::discriminant(&reader.kind) {
                    let (a, b) = (code_children(lowered), code_children(reader));
                    if a.len() == b.len() {
                        pending.extend(a.iter().zip(b));
                    }
                }
            }
        }
        Ok(self
            .original_form_data(form, &mut budget)?
            .unwrap_or_else(|| form.clone()))
    }

    pub(crate) fn with_reader_expansion(&self, lowered: Form, reader: Form) -> Self {
        let mut result = self.clone();
        result.reader_forms = Some(std::sync::Arc::new((lowered, reader)));
        result
    }

    /// Synthesized defmacro wrappers may not match source. Enrich only verified
    /// original subtrees and leave every compiler-generated location unknown.
    pub(crate) fn reader_tree(&self, form: &Form) -> Result<Form, Diagnostic> {
        fn enrich(
            origin: &SourceOrigin,
            form: &Form,
            depth: usize,
            budget: &mut usize,
        ) -> Result<Form, Diagnostic> {
            if depth >= 64 || *budget == 0 {
                return Err(Diagnostic {
                    span: form.span.clone(),
                    message: "Reader source provenance exceeds traversal bound".into(),
                });
            }
            *budget -= 1;
            if let Some(original) = origin.original_form_data(form, budget)? {
                return Ok(original);
            }
            let mut result = form.clone();
            result.metadata = form
                .metadata
                .iter()
                .map(|f| enrich(origin, f, depth + 1, budget))
                .collect::<Result<_, _>>()?;
            match &form.kind {
                Kind::List(items) | Kind::Vector(items) | Kind::Map(items) | Kind::Set(items) => {
                    let items = items
                        .iter()
                        .map(|f| enrich(origin, f, depth + 1, budget))
                        .collect::<Result<_, _>>()?;
                    result.kind = match form.kind {
                        Kind::List(_) => Kind::List(items),
                        Kind::Vector(_) => Kind::Vector(items),
                        Kind::Map(_) => Kind::Map(items),
                        _ => Kind::Set(items),
                    };
                }
                _ => (),
            }
            Ok(result)
        }
        enrich(self, form, 0, &mut 1_048_576)
    }

    fn original_form_data(
        &self,
        form: &Form,
        budget: &mut usize,
    ) -> Result<Option<Form>, Diagnostic> {
        if self.text.len() > 1_048_576 {
            return Err(Diagnostic {
                span: form.span.clone(),
                message: "Macro source metadata snapshot exceeds 1 MiB".into(),
            });
        }
        let forms = self.metadata_forms.get_or_init(|| {
            let path = self
                .path
                .as_ref()
                .map(|path| {
                    path.to_str()
                        .ok_or_else(|| "Source metadata path is not Unicode".to_owned())
                })
                .transpose()?;
            let original =
                resolve_conditionals(read_forms(&self.text).map_err(|error| error.to_string())?)
                    .map_err(|error| error.to_string())?;
            let located = read_forms_with_source_metadata(&self.text, path)
                .map_err(|error| error.to_string())?;
            Ok((original, located))
        });
        let (original, located) = forms.as_ref().map_err(|error| Diagnostic {
            span: form.span.clone(),
            message: format!("Unable to construct source metadata snapshot: {error}"),
        })?;
        if find(original, form, true, budget)?.is_none() {
            return Ok(None);
        }
        find(located, form, false, budget)?
            .cloned()
            .map(Some)
            .ok_or_else(|| Diagnostic {
                span: form.span.clone(),
                message: "Verified source form missing from source metadata snapshot".into(),
            })
    }
}
