//! Original provenance matching for opt-in indexing-reader macro data.
use super::SourceOrigin;
use crate::portable::Diagnostic;
use suss_reader::forms::{
    read_forms, read_forms_with_source_metadata, resolve_conditionals, Form, Kind,
};

fn children(form: &Form) -> impl Iterator<Item = &Form> {
    let items = match &form.kind {
        Kind::List(items) | Kind::Vector(items) | Kind::Map(items) | Kind::Set(items) => {
            items.as_slice()
        }
        _ => &[],
    };
    form.metadata.iter().chain(items)
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
        let mut budget = 1_048_576;
        if find(original, form, true, &mut budget)?.is_none() {
            return Ok(form.clone());
        }
        find(located, form, false, &mut budget)?
            .cloned()
            .ok_or_else(|| Diagnostic {
                span: form.span.clone(),
                message: "Verified source form missing from source metadata snapshot".into(),
            })
    }
}
