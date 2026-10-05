//! Original provenance matching for opt-in indexing-reader macro data.
use super::SourceOrigin;
use crate::portable::Diagnostic;
use std::collections::HashMap;
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

#[cfg(test)]
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

const TRAVERSAL_BOUND: usize = 1_048_576;

#[derive(Debug)]
pub(super) struct MetadataSnapshot {
    original: IndexedForms,
    located: IndexedForms,
}

#[derive(Debug)]
struct IndexedForms {
    forms: Vec<Form>,
    index: FormIndex,
}

impl IndexedForms {
    fn new(forms: Vec<Form>) -> Self {
        let index = FormIndex::new(&forms, TRAVERSAL_BOUND);
        Self { forms, index }
    }

    fn find(
        &self,
        wanted: &Form,
        metadata: bool,
        budget: &mut usize,
    ) -> Result<Option<&Form>, Diagnostic> {
        self.index.find(&self.forms, wanted, metadata, budget)
    }
}

#[derive(Debug, Clone, Copy)]
enum Step {
    Root(usize),
    Metadata(usize),
    Code(usize),
}

#[derive(Debug)]
struct Node {
    parent: Option<usize>,
    step: Step,
}

/// Immutable paths into the owned trees, in the linear matcher's visitation
/// order. Nodes contain no cloned subtrees or references into movable storage.
/// Only the bounded prefix reachable by a provenance query is indexed.
#[derive(Debug)]
struct FormIndex {
    nodes: Vec<Node>,
    spans: HashMap<(usize, usize), Vec<usize>>,
    truncated: bool,
    limit: usize,
}

impl FormIndex {
    fn new(forms: &[Form], limit: usize) -> Self {
        let mut pending: Vec<_> = forms
            .iter()
            .enumerate()
            .rev()
            .map(|(i, form)| (form, None, Step::Root(i)))
            .collect();
        let mut nodes = Vec::new();
        let mut spans: HashMap<_, Vec<_>> = HashMap::new();
        while nodes.len() < limit {
            let Some((form, parent, step)) = pending.pop() else {
                break;
            };
            let rank = nodes.len();
            nodes.push(Node { parent, step });
            spans
                .entry((form.span.start, form.span.end))
                .or_default()
                .push(rank);
            // Preserve the old stack order: code children are visited in
            // reverse, followed by metadata children in reverse.
            pending.extend(
                form.metadata
                    .iter()
                    .enumerate()
                    .map(|(i, form)| (form, Some(rank), Step::Metadata(i))),
            );
            pending.extend(
                code_children(form)
                    .iter()
                    .enumerate()
                    .map(|(i, form)| (form, Some(rank), Step::Code(i))),
            );
        }
        Self {
            nodes,
            spans,
            truncated: !pending.is_empty(),
            limit,
        }
    }

    fn form<'a>(&self, forms: &'a [Form], mut rank: usize) -> &'a Form {
        let mut steps = Vec::new();
        let root = loop {
            let node = &self.nodes[rank];
            match node.parent {
                Some(parent) => {
                    steps.push(node.step);
                    rank = parent;
                }
                None => {
                    let Step::Root(root) = node.step else {
                        unreachable!()
                    };
                    break root;
                }
            }
        };
        let mut form = &forms[root];
        for step in steps.into_iter().rev() {
            form = match step {
                Step::Metadata(i) => &form.metadata[i],
                Step::Code(i) => &code_children(form)[i],
                Step::Root(_) => unreachable!(),
            };
        }
        form
    }

    fn find<'a>(
        &self,
        forms: &'a [Form],
        wanted: &Form,
        metadata: bool,
        budget: &mut usize,
    ) -> Result<Option<&'a Form>, Diagnostic> {
        // Public queries start with this bound, or a remainder of it. Charging
        // logical visits keeps shared reader-tree budgets and errors unchanged.
        debug_assert!(*budget <= self.limit);
        let failure = || Diagnostic {
            span: wanted.span.clone(),
            message: "Source metadata provenance exceeds traversal bound".into(),
        };
        let mut charge = |count| -> Result<(), Diagnostic> {
            if let Some(remaining) = budget.checked_sub(count) {
                *budget = remaining;
                Ok(())
            } else {
                *budget = 0;
                Err(failure())
            }
        };
        let mut visited = 0;
        if let Some(candidates) = self.spans.get(&(wanted.span.start, wanted.span.end)) {
            for &rank in candidates {
                charge(rank + 1 - visited)?;
                visited = rank + 1;
                let form = self.form(forms, rank);
                if same(form, wanted, metadata) {
                    return Ok(Some(form));
                }
            }
        }
        charge(self.nodes.len() - visited)?;
        if self.truncated {
            charge(1)?;
        }
        Ok(None)
    }
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
            Ok(MetadataSnapshot {
                original: IndexedForms::new(original),
                located: IndexedForms::new(located),
            })
        });
        let snapshot = forms.as_ref().map_err(|error| Diagnostic {
            span: form.span.clone(),
            message: format!("Unable to construct source metadata snapshot: {error}"),
        })?;
        if snapshot.original.find(form, true, budget)?.is_none() {
            return Ok(None);
        }
        snapshot
            .located
            .find(form, false, budget)?
            .cloned()
            .map(Some)
            .ok_or_else(|| Diagnostic {
                span: form.span.clone(),
                message: "Verified source form missing from source metadata snapshot".into(),
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all_forms(forms: &[Form]) -> Vec<&Form> {
        let mut pending: Vec<_> = forms.iter().rev().collect();
        let mut result = Vec::new();
        while let Some(form) = pending.pop() {
            result.push(form);
            pending.extend(children(form));
        }
        result
    }

    fn equivalent(forms: &[Form], index: &FormIndex, wanted: &Form, metadata: bool, budget: usize) {
        let (mut linear_budget, mut indexed_budget) = (budget, budget);
        let linear = find(forms, wanted, metadata, &mut linear_budget);
        let indexed = index.find(forms, wanted, metadata, &mut indexed_budget);
        assert_eq!(
            linear_budget, indexed_budget,
            "remaining budget for {wanted:?}"
        );
        match (linear, indexed) {
            (Ok(Some(a)), Ok(Some(b))) => {
                // Pointer identity verifies first-match order, including forms
                // which have identical spans, metadata and syntax.
                assert!(std::ptr::eq(a, b), "different first match for {wanted:?}");
            }
            (Ok(None), Ok(None)) => (),
            (Err(a), Err(b)) => {
                assert_eq!(a.span, b.span);
                assert_eq!(a.message, b.message);
            }
            (a, b) => panic!("linear/indexed mismatch for {wanted:?}: {a:?} / {b:?}"),
        }
    }

    #[test]
    fn provenance_index_matches_linear_reader_trees_and_budget_boundaries() {
        let source = "^:outer (do [1 -0.0 ##NaN] {:a #{2 3} :b ^{:tag number} value})\n#?(:suss (quote x) :cljs nope)";
        let original = resolve_conditionals(read_forms(source).unwrap()).unwrap();
        let located = read_forms_with_source_metadata(source, Some("source.cljc")).unwrap();
        for forms in [&original, &located] {
            let all = all_forms(forms);
            let limit = all.len() + 2;
            let index = FormIndex::new(forms, limit);
            for form in all {
                let mut changed = form.clone();
                changed.kind = Kind::String(vec![0xd83d, 0, 0xdc00]);
                let mut changed_metadata = form.clone();
                changed_metadata.metadata.push(Form {
                    span: 0..0,
                    metadata: Vec::new(),
                    kind: Kind::Bool(false),
                });
                for wanted in [form, &changed, &changed_metadata] {
                    for metadata in [false, true] {
                        for budget in 0..=limit {
                            equivalent(forms, &index, wanted, metadata, budget);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn provenance_index_preserves_colliding_spans_float_bits_and_child_order() {
        let leaf = |number| Form {
            span: 0..1,
            metadata: Vec::new(),
            kind: Kind::Number(number),
        };
        let forms = vec![
            Form {
                span: 0..1,
                metadata: vec![leaf(0.0), leaf(-0.0)],
                kind: Kind::Vector(vec![
                    leaf(f64::from_bits(0x7ff8000000000001)),
                    leaf(-0.0),
                    leaf(-0.0),
                ]),
            },
            leaf(f64::from_bits(0x7ff8000000000002)),
        ];
        let index = FormIndex::new(&forms, 20);
        for form in all_forms(&forms) {
            for budget in 0..=20 {
                equivalent(&forms, &index, form, true, budget);
                equivalent(&forms, &index, form, false, budget);
            }
        }
        let absent_nan = leaf(f64::from_bits(0xfff8000000000001));
        for budget in 0..=20 {
            equivalent(&forms, &index, &absent_nan, true, budget);
        }
    }

    #[test]
    fn provenance_index_truncation_preserves_early_matches_and_exhaustion() {
        let forms = read_forms("[0 [1 2] 3] 4 5").unwrap();
        let absent = Form {
            span: 100..101,
            metadata: Vec::new(),
            kind: Kind::Nil,
        };
        for limit in 0..=all_forms(&forms).len() {
            let index = FormIndex::new(&forms, limit);
            for wanted in all_forms(&forms).into_iter().chain([&absent]) {
                for budget in 0..=limit {
                    equivalent(&forms, &index, wanted, true, budget);
                }
            }
        }
        let index = FormIndex::new(&[], 0);
        equivalent(&[], &index, &absent, true, 0);
    }

    #[test]
    fn provenance_index_late_unique_span_has_one_candidate() {
        let source = (0..10_000)
            .map(|i| i.to_string())
            .collect::<Vec<_>>()
            .join(" ");
        let forms = read_forms(&source).unwrap();
        let index = FormIndex::new(&forms, TRAVERSAL_BOUND);
        let wanted = forms.last().unwrap();
        assert_eq!(
            index.spans[&(wanted.span.start, wanted.span.end)],
            vec![9_999]
        );
        let mut budget = TRAVERSAL_BOUND;
        assert!(std::ptr::eq(
            index
                .find(&forms, wanted, true, &mut budget)
                .unwrap()
                .unwrap(),
            wanted
        ));
        assert_eq!(budget, TRAVERSAL_BOUND - 10_000);
        equivalent(&forms, &index, wanted, true, 10_000);
        equivalent(&forms, &index, wanted, true, 9_999);
    }

    #[test]
    fn provenance_index_snapshot_keeps_shared_original_and_located_budget() {
        let source = "^:marked (identity [false nil -0.0]) {:key \"value\"}";
        let original = resolve_conditionals(read_forms(source).unwrap()).unwrap();
        let located = read_forms_with_source_metadata(source, Some("snapshot.sus")).unwrap();
        let origin = SourceOrigin::new(source, Some("snapshot.sus".into()));
        let clone = origin.clone();
        let absent = Form {
            span: 0..source.len(),
            metadata: Vec::new(),
            kind: Kind::Nil,
        };
        for wanted in all_forms(&original).into_iter().chain([&absent]) {
            for budget in 0..=all_forms(&original).len() + all_forms(&located).len() + 1 {
                let (mut old_budget, mut new_budget) = (budget, budget);
                let old = (|| {
                    if find(&original, wanted, true, &mut old_budget)?.is_none() {
                        return Ok(None);
                    }
                    find(&located, wanted, false, &mut old_budget)?
                        .cloned()
                        .map(Some)
                        .ok_or_else(|| Diagnostic {
                            span: wanted.span.clone(),
                            message: "Verified source form missing from source metadata snapshot"
                                .into(),
                        })
                })();
                let new = clone.original_form_data(wanted, &mut new_budget);
                assert_eq!(old_budget, new_budget);
                match (old, new) {
                    (Ok(Some(a)), Ok(Some(b))) => assert!(same(&a, &b, true)),
                    (Ok(None), Ok(None)) => (),
                    (Err(a), Err(b)) => {
                        assert_eq!(a.span, b.span);
                        assert_eq!(a.message, b.message);
                    }
                    (a, b) => panic!("snapshot mismatch for {wanted:?}: {a:?} / {b:?}"),
                }
            }
        }
        assert!(std::sync::Arc::ptr_eq(
            &origin.metadata_forms,
            &clone.metadata_forms
        ));
        assert!(origin.metadata_forms.get().unwrap().is_ok());
    }
}
