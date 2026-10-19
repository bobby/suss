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

type WorkMeter<'a> = dyn FnMut(u64) -> Result<(), Diagnostic> + 'a;

fn charge(
    meter: &mut WorkMeter<'_>,
    amount: usize,
    span: &std::ops::Range<usize>,
) -> Result<(), Diagnostic> {
    if amount == 0 {
        return Ok(());
    }
    let amount = u64::try_from(amount).unwrap_or(u64::MAX);
    meter(amount).map_err(|mut error| {
        error.span = span.clone();
        error
    })
}

fn all_children(form: &Form) -> Vec<&Form> {
    let mut result: Vec<_> = form.metadata.iter().collect();
    match &form.kind {
        Kind::List(items)
        | Kind::Vector(items)
        | Kind::Map(items)
        | Kind::Set(items)
        | Kind::Conditional(items)
        | Kind::SplicingConditional(items) => result.extend(items),
        Kind::Discard(item) => result.push(item),
        Kind::Prefix { operator, target } => result.extend([operator.as_ref(), target.as_ref()]),
        _ => {}
    }
    result
}

#[derive(Clone, Copy)]
enum CompareMode {
    Same(bool),
    Derived,
}

/// Iterative equivalent of `same` that meters both node visits and compared
/// text. `Derived` is used only where the legacy fallback delegated to Form's
/// derived PartialEq implementation.
fn same_metered(
    left: &Form,
    right: &Form,
    metadata: bool,
    meter: &mut WorkMeter<'_>,
) -> Result<bool, Diagnostic> {
    let mut pending = vec![(left, right, CompareMode::Same(metadata))];
    while let Some((a, b, mode)) = pending.pop() {
        charge(meter, 1, &a.span)?;
        if a.span != b.span {
            return Ok(false);
        }
        let derived = matches!(mode, CompareMode::Derived);
        if derived || matches!(mode, CompareMode::Same(true)) {
            if a.metadata.len() != b.metadata.len() {
                return Ok(false);
            }
            pending.extend(a.metadata.iter().zip(&b.metadata).rev().map(|(x, y)| {
                (
                    x,
                    y,
                    if derived {
                        CompareMode::Derived
                    } else {
                        CompareMode::Same(true)
                    },
                )
            }));
        }
        let equal = match (&a.kind, &b.kind) {
            (Kind::Nil, Kind::Nil) => true,
            (Kind::Bool(x), Kind::Bool(y)) => x == y,
            (Kind::Number(x), Kind::Number(y)) => {
                if derived {
                    x == y
                } else {
                    x.to_bits() == y.to_bits()
                }
            }
            (Kind::String(x), Kind::String(y)) => {
                charge(meter, x.len().max(y.len()), &a.span)?;
                x == y
            }
            (Kind::Symbol(x), Kind::Symbol(y)) => {
                charge(
                    meter,
                    x.name.len().max(y.name.len())
                        + x.namespace
                            .as_ref()
                            .map_or(0, String::len)
                            .max(y.namespace.as_ref().map_or(0, String::len)),
                    &a.span,
                )?;
                x == y
            }
            (Kind::Keyword(x), Kind::Keyword(y)) => {
                charge(
                    meter,
                    x.name.len().max(y.name.len())
                        + x.namespace
                            .as_ref()
                            .map_or(0, String::len)
                            .max(y.namespace.as_ref().map_or(0, String::len)),
                    &a.span,
                )?;
                x == y
            }
            (Kind::List(x), Kind::List(y))
            | (Kind::Vector(x), Kind::Vector(y))
            | (Kind::Map(x), Kind::Map(y))
            | (Kind::Set(x), Kind::Set(y))
                if x.len() == y.len() =>
            {
                pending.extend(x.iter().zip(y).rev().map(|(x, y)| (x, y, mode)));
                true
            }
            (Kind::Conditional(x), Kind::Conditional(y))
            | (Kind::SplicingConditional(x), Kind::SplicingConditional(y))
                if x.len() == y.len() =>
            {
                pending.extend(
                    x.iter()
                        .zip(y)
                        .rev()
                        .map(|(x, y)| (x, y, CompareMode::Derived)),
                );
                true
            }
            (Kind::Discard(x), Kind::Discard(y)) => {
                pending.push((x, y, CompareMode::Derived));
                true
            }
            (
                Kind::Prefix {
                    operator: xo,
                    target: xt,
                },
                Kind::Prefix {
                    operator: yo,
                    target: yt,
                },
            ) => {
                pending.push((xt, yt, CompareMode::Derived));
                pending.push((xo, yo, CompareMode::Derived));
                true
            }
            _ => false,
        };
        if !equal {
            return Ok(false);
        }
    }
    Ok(true)
}

fn form_text_bytes(form: &Form) -> usize {
    match &form.kind {
        Kind::String(value) => value.len().saturating_mul(2),
        Kind::Symbol(value) => value
            .name
            .len()
            .saturating_add(value.namespace.as_ref().map_or(0, String::len)),
        Kind::Keyword(value) => value
            .name
            .len()
            .saturating_add(value.namespace.as_ref().map_or(0, String::len)),
        _ => 0,
    }
}

fn rebuild_form(node: &Form, children: Vec<Form>) -> Form {
    let mut children = children.into_iter();
    let metadata = children
        .by_ref()
        .take(node.metadata.len())
        .collect::<Vec<_>>();
    let values = children.collect::<Vec<_>>();
    let kind = match &node.kind {
        Kind::Nil => Kind::Nil,
        Kind::Bool(value) => Kind::Bool(*value),
        Kind::Number(value) => Kind::Number(*value),
        Kind::String(value) => Kind::String(value.clone()),
        Kind::Symbol(value) => Kind::Symbol(value.clone()),
        Kind::Keyword(value) => Kind::Keyword(value.clone()),
        Kind::List(_) => Kind::List(values),
        Kind::Vector(_) => Kind::Vector(values),
        Kind::Map(_) => Kind::Map(values),
        Kind::Set(_) => Kind::Set(values),
        Kind::Conditional(_) => Kind::Conditional(values),
        Kind::SplicingConditional(_) => Kind::SplicingConditional(values),
        Kind::Discard(_) => Kind::Discard(Box::new(values.into_iter().next().unwrap())),
        Kind::Prefix { .. } => {
            let mut values = values.into_iter();
            Kind::Prefix {
                operator: Box::new(values.next().unwrap()),
                target: Box::new(values.next().unwrap()),
            }
        }
    };
    Form {
        span: node.span.clone(),
        metadata,
        kind,
    }
}

fn clone_work_size(count: usize, size: usize, span: &std::ops::Range<usize>) -> Result<usize, Diagnostic> {
    count.checked_mul(size).ok_or_else(|| Diagnostic { span: span.clone(), message: "Form clone work overflow".into() })
}
fn clone_reserve<T>(values: &mut Vec<T>, additional: usize, meter: &mut WorkMeter<'_>, span: &std::ops::Range<usize>) -> Result<(), Diagnostic> {
    let required = values.len().checked_add(additional).ok_or_else(|| Diagnostic { span: span.clone(), message: "Form clone capacity overflow".into() })?;
    if required <= values.capacity() { return Ok(()); }
    // Explicit growth: charge requested backing plus relocation before reserve.
    let target = values.capacity().checked_mul(2).map(|n| n.max(required)).ok_or_else(|| Diagnostic { span: span.clone(), message: "Form clone capacity overflow".into() })?;
    charge(meter, clone_work_size(target, std::mem::size_of::<T>(), span)?, span)?;
    charge(meter, clone_work_size(values.len(), std::mem::size_of::<T>(), span)?, span)?;
    values.try_reserve_exact(target - values.len()).map_err(|_| Diagnostic { span: span.clone(), message: "Unable to reserve form clone storage".into() })
}
fn clone_code_count(node: &Form) -> usize {
    match &node.kind {
        Kind::List(items) | Kind::Vector(items) | Kind::Map(items) | Kind::Set(items)
        | Kind::Conditional(items) | Kind::SplicingConditional(items) => items.len(),
        Kind::Discard(_) => 1,
        Kind::Prefix { .. } => 2,
        _ => 0,
    }
}
fn clone_source_child(node: &Form, index: usize) -> &Form {
    if index < node.metadata.len() { return &node.metadata[index]; }
    let index = index - node.metadata.len();
    match &node.kind {
        Kind::List(items) | Kind::Vector(items) | Kind::Map(items) | Kind::Set(items)
        | Kind::Conditional(items) | Kind::SplicingConditional(items) => &items[index],
        Kind::Discard(item) => { assert_eq!(index, 0); item },
        Kind::Prefix { operator, target } => if index == 0 { operator } else { assert_eq!(index, 1); target },
        _ => unreachable!("atomic forms have no code children"),
    }
}
struct FormCloneFrame<'a> {
    node: &'a Form,
    next: usize,
    count: usize,
    metadata: Vec<Form>,
    values: Vec<Form>,
}
fn new_clone_frame<'a>(node: &'a Form, meter: &mut WorkMeter<'_>) -> Result<FormCloneFrame<'a>, Diagnostic> {
    charge(meter, 1, &node.span)?;
    charge(meter, form_text_bytes(node), &node.span)?;
    let code_count = clone_code_count(node);
    let count = node.metadata.len().checked_add(code_count).ok_or_else(|| Diagnostic {
        span: node.span.clone(), message: "Form clone child count overflow".into(),
    })?;
    charge(meter, count, &node.span)?;
    // These are the final output containers, not intermediate child storage.
    let mut metadata = Vec::new();
    clone_reserve(&mut metadata, node.metadata.len(), meter, &node.span)?;
    let mut values = Vec::new();
    clone_reserve(&mut values, code_count, meter, &node.span)?;
    Ok(FormCloneFrame { node, next: 0, count, metadata, values })
}
fn finish_cloned_form(node: &Form, metadata: Vec<Form>, values: Vec<Form>, meter: &mut WorkMeter<'_>) -> Result<Form, Diagnostic> {
    let kind = match &node.kind {
        Kind::Nil => Kind::Nil,
        Kind::Bool(value) => Kind::Bool(*value),
        Kind::Number(value) => Kind::Number(*value),
        Kind::String(value) => {
            let mut copied = Vec::new();
            clone_reserve(&mut copied, value.len(), meter, &node.span)?;
            copied.extend_from_slice(value);
            Kind::String(copied)
        }
        Kind::Symbol(value) => Kind::Symbol(value.clone()),
        Kind::Keyword(value) => Kind::Keyword(value.clone()),
        Kind::List(_) => Kind::List(values),
        Kind::Vector(_) => Kind::Vector(values),
        Kind::Map(_) => Kind::Map(values),
        Kind::Set(_) => Kind::Set(values),
        Kind::Conditional(_) => Kind::Conditional(values),
        Kind::SplicingConditional(_) => Kind::SplicingConditional(values),
        Kind::Discard(_) => {
            charge(meter, std::mem::size_of::<Form>(), &node.span)?;
            Kind::Discard(Box::new(values.into_iter().next().unwrap()))
        },
        Kind::Prefix { .. } => {
            charge(meter, clone_work_size(2, std::mem::size_of::<Form>(), &node.span)?, &node.span)?;
            let mut values = values.into_iter();
            Kind::Prefix { operator: Box::new(values.next().unwrap()), target: Box::new(values.next().unwrap()) }
        }
    };
    Ok(Form { span: node.span.clone(), metadata, kind })
}
fn clone_is_atomic(node: &Form) -> bool {
    node.metadata.is_empty() && matches!(&node.kind,
        Kind::Nil | Kind::Bool(_) | Kind::Number(_) | Kind::String(_) | Kind::Symbol(_) | Kind::Keyword(_))
}
fn clone_atomic(node: &Form, meter: &mut WorkMeter<'_>) -> Result<Form, Diagnostic> {
    charge(meter, 1, &node.span)?;
    charge(meter, form_text_bytes(node), &node.span)?;
    finish_cloned_form(node, Vec::new(), Vec::new(), meter)
}
fn attach_clone(parent: &mut FormCloneFrame<'_>, child: Form) {
    // next already advanced when this child was selected. Every push uses its
    // parent's precharged final backing; no append can allocate or relocate.
    if parent.next <= parent.node.metadata.len() { parent.metadata.push(child); }
    else { parent.values.push(child); }
}

pub(crate) fn clone_form_metered(form: &Form, meter: &mut WorkMeter<'_>) -> Result<Form, Diagnostic> {
    let _trace = crate::portable::hir::CowTraceScope::new("form_copy");
    if clone_is_atomic(form) { return clone_atomic(form, meter); }
    let root = new_clone_frame(form, meter)?;
    let mut frames = Vec::new();
    clone_reserve(&mut frames, 1, meter, &form.span)?;
    frames.push(root);
    loop {
        let parent = frames.last_mut().expect("unfinished clone root");
        if parent.next < parent.count {
            // Index source children in metadata-then-code order. No Vec<&Form>
            // is built, and finished Forms go directly to their parent's output.
            let child = clone_source_child(parent.node, parent.next);
            parent.next += 1;
            if clone_is_atomic(child) {
                let copied = clone_atomic(child, meter)?;
                attach_clone(frames.last_mut().unwrap(), copied);
            } else {
                let frame = new_clone_frame(child, meter)?;
                clone_reserve(&mut frames, 1, meter, &child.span)?;
                frames.push(frame);
            }
        } else {
            let frame = frames.pop().unwrap();
            let copied = finish_cloned_form(frame.node, frame.metadata, frame.values, meter)?;
            if let Some(parent) = frames.last_mut() { attach_clone(parent, copied); }
            else { return Ok(copied); }
        }
    }
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
    source: suss_reader::forms::SourceMetadata,
    index: FormIndex,
    located: std::sync::OnceLock<IndexedForms>,
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

    fn new_metered(forms: Vec<Form>, meter: &mut WorkMeter<'_>) -> Result<Self, Diagnostic> {
        let index = FormIndex::new_metered(&forms, meter)?;
        Ok(Self { forms, index })
    }

    fn find_metered(
        &self,
        wanted: &Form,
        metadata: bool,
        meter: &mut WorkMeter<'_>,
    ) -> Result<Option<&Form>, Diagnostic> {
        self.index
            .find_metered(&self.forms, wanted, metadata, meter)
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

    fn new_metered(forms: &[Form], meter: &mut WorkMeter<'_>) -> Result<Self, Diagnostic> {
        let mut pending: Vec<_> = forms
            .iter()
            .enumerate()
            .rev()
            .map(|(i, form)| (form, None, Step::Root(i)))
            .collect();
        let mut nodes = Vec::new();
        let mut spans: HashMap<_, Vec<usize>> = HashMap::new();
        while let Some((form, parent, step)) = pending.pop() {
            let children = children(form).collect::<Vec<_>>();
            charge(meter, 1usize.saturating_add(children.len()), &form.span)?;
            let rank = nodes.len();
            nodes.push(Node { parent, step });
            spans
                .entry((form.span.start, form.span.end))
                .or_default()
                .push(rank);
            pending.extend(
                form.metadata
                    .iter()
                    .enumerate()
                    .map(|(i, child)| (child, Some(rank), Step::Metadata(i))),
            );
            pending.extend(
                code_children(form)
                    .iter()
                    .enumerate()
                    .map(|(i, child)| (child, Some(rank), Step::Code(i))),
            );
        }
        Ok(Self {
            nodes,
            spans,
            truncated: false,
            limit: usize::MAX,
        })
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

    fn form_metered<'a>(
        &self,
        forms: &'a [Form],
        mut rank: usize,
        meter: &mut WorkMeter<'_>,
        span: &std::ops::Range<usize>,
    ) -> Result<&'a Form, Diagnostic> {
        let mut steps = Vec::new();
        let root = loop {
            charge(meter, 1, span)?;
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
            charge(meter, 1, span)?;
            form = match step {
                Step::Metadata(i) => &form.metadata[i],
                Step::Code(i) => &code_children(form)[i],
                Step::Root(_) => unreachable!(),
            };
        }
        Ok(form)
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

    fn find_metered<'a>(
        &self,
        forms: &'a [Form],
        wanted: &Form,
        metadata: bool,
        meter: &mut WorkMeter<'_>,
    ) -> Result<Option<&'a Form>, Diagnostic> {
        charge(meter, 1, &wanted.span)?; // span-index lookup
        if let Some(candidates) = self.spans.get(&(wanted.span.start, wanted.span.end)) {
            for &rank in candidates {
                charge(meter, 1, &wanted.span)?;
                let candidate = self.form_metered(forms, rank, meter, &wanted.span)?;
                if same_metered(candidate, wanted, metadata, meter)? {
                    return Ok(Some(candidate));
                }
            }
        }
        Ok(None)
    }
}

enum MacroFormSource<'a> {
    Borrowed(&'a Form),
    Original(suss_reader::forms::SourceMetadataNode<'a>),
}
fn demand_read<T>(meter:&mut WorkMeter<'_>,run:impl FnOnce(&mut suss_reader::forms::WorkMeter<'_>)->Result<T,suss_reader::ParseError>)->Result<T,Diagnostic> {
    let mut reader_meter=|work|meter(work).map_err(|error|suss_reader::ParseError {
        span:error.span,message:error.message,expected:vec![],
    });
    run(&mut reader_meter).map_err(|error|Diagnostic{span:error.span,message:error.message})
}
impl FormIndex {
    fn find_source_metered<'a>(&self,source:&'a suss_reader::forms::SourceMetadata,wanted:&Form,meter:&mut WorkMeter<'_>)
        ->Result<Option<suss_reader::forms::SourceMetadataNode<'a>>,Diagnostic> {
        charge(meter,1,&wanted.span)?;
        if let Some(candidates)=self.spans.get(&(wanted.span.start,wanted.span.end)) {
            for &rank in candidates {
                charge(meter,1,&wanted.span)?;
                let mut rank=rank;
                let mut steps=Vec::new();
                let root=loop {
                    charge(meter,2,&wanted.span)?;
                    let node=&self.nodes[rank];
                    if let Some(parent)=node.parent {
                        if steps.len()==steps.capacity() {
                            let capacity=steps.capacity().saturating_mul(2).max(4);
                            charge(meter,(capacity+steps.len())*std::mem::size_of::<Step>(),&wanted.span)?;
                            steps.reserve_exact(capacity-steps.len());
                        }
                        steps.push(node.step); rank=parent;
                    } else {let Step::Root(root)=node.step else {unreachable!()};break root;}
                };
                let mut node=source.root(root).expect("original source root");
                for step in steps.into_iter().rev() {
                    charge(meter,2,&wanted.span)?;
                    node=match step {Step::Metadata(i)=>node.child(true,i),Step::Code(i)=>node.child(false,i),Step::Root(_)=>unreachable!()}.expect("original source path");
                }
                if same_metered(node.form(),wanted,true,meter)? {return Ok(Some(node));}
            }
        }
        Ok(None)
    }
}
impl SourceOrigin {
    /// Reader metadata for a source form whose entire original syntax and spans
    /// still match. Expanded data with reused spans retains only explicit meta.
    /// Neither provenance construction nor comparison evaluates source.
    pub fn macro_form_data(&self, form: &Form) -> Result<Form, Diagnostic> {
        let mut remaining = 1_048_576u64;
        self.macro_form_data_metered(form, &mut |work| {
            remaining = remaining.checked_sub(work).ok_or_else(|| Diagnostic {
                span: form.span.clone(),
                message: "Source provenance computation fuel exhausted".into(),
            })?;
            Ok(())
        })
    }

    /// Metered source-form provenance for macro expansion. The callback is
    /// charged for input bytes parsed, indexed/tree nodes visited, comparisons,
    /// and copied form data. Warm snapshots still charge every lookup and clone.
    /// The legacy `macro_form_data` wrapper retains a finite allowance for
    /// callers that do not provide the session's operation meter.
    pub(crate) fn macro_form_data_metered(
        &self, form: &Form,
        meter: &mut impl FnMut(u64) -> Result<(), Diagnostic>,
    ) -> Result<Form, Diagnostic> {
        match self.macro_form_source_metered(form,meter)? {
            Some(MacroFormSource::Original(node))=>{
                if std::env::var_os("SUSS_METADATA_FUEL_TRACE").is_some() {eprintln!("origin-demand mode=full composite={} span={}:{}",matches!(node.form().kind,Kind::List(_)|Kind::Vector(_)|Kind::Map(_)|Kind::Set(_)),form.span.start,form.span.end);}
                demand_read(meter,|meter|node.located_metered(meter))
            }
            source=>clone_form_metered(match source {Some(MacroFormSource::Borrowed(form))=>form,None=>form,_=>unreachable!()},meter),
        }
    }

    /// Reader declaration metadata provenance for the actual clone operation.
    /// For a verified original Symbol, clone_attached copies the original root
    /// prefixes in place/order then appends indexing metadata. This count is
    /// recorded BEFORE that clone; no output key/content/address join is used.
    /// Borrowed expanded/eager mappings are explicitly unclassified here.
    pub(crate) fn macro_form_data_with_metadata_origins_metered(
        &self, form: &Form,
        meter: &mut impl FnMut(u64) -> Result<(), Diagnostic>,
    ) -> Result<(Form, Option<usize>), Diagnostic> {
        charge(meter, 8, &form.span)?;
        match self.macro_form_source_metered(form, meter)? {
            Some(MacroFormSource::Original(node)) => {
                let prefixes = if matches!(&node.form().kind, Kind::Symbol(_)) && !node.is_caret_prefix() {
                    Some(node.form().metadata.len())
                } else { None };
                if std::env::var_os("SUSS_METADATA_FUEL_TRACE").is_some() {eprintln!("origin-demand mode=full composite={} span={}:{}",matches!(node.form().kind,Kind::List(_)|Kind::Vector(_)|Kind::Map(_)|Kind::Set(_)),form.span.start,form.span.end);}
                let copied = demand_read(meter, |meter| node.located_metered(meter))?;
                Ok((copied, prefixes))
            }
            Some(MacroFormSource::Borrowed(reader)) => {
                Ok((clone_form_metered(reader, meter)?, None))
            }
            None => {
                let prefixes = matches!(&form.kind, Kind::Symbol(_)).then_some(form.metadata.len());
                Ok((clone_form_metered(form, meter)?, prefixes))
            }
        }
    }

    /// Root metadata with the same complete provenance checks as full reader
    /// data. The selected span is retained for normalization diagnostics.
    pub(crate) fn macro_form_root_metadata_metered(
        &self, form: &Form,
        meter: &mut impl FnMut(u64) -> Result<(), Diagnostic>,
    ) -> Result<(std::ops::Range<usize>, Vec<Form>), Diagnostic> {
        let selected=match self.macro_form_source_metered(form,meter)? {
            Some(MacroFormSource::Original(node))=>{
                if std::env::var_os("SUSS_METADATA_FUEL_TRACE").is_some() {eprintln!("origin-demand mode=root span={}:{}",form.span.start,form.span.end);}
                return demand_read(meter,|meter|node.root_metadata_metered(meter));
            }
            Some(MacroFormSource::Borrowed(form))=>form,None=>form,
        };
        let mut metadata = Vec::new();
        charge(meter, selected.metadata.len().saturating_mul(std::mem::size_of::<Form>()), &selected.span)?;
        metadata.try_reserve_exact(selected.metadata.len()).map_err(|_| Diagnostic {
            span: selected.span.clone(), message: "Source root metadata allocation failed".into(),
        })?;
        for prefix in &selected.metadata {
            charge(meter, 1, &selected.span)?;
            metadata.push(clone_form_metered(prefix, meter)?);
        }
        Ok((selected.span.clone(), metadata))
    }

    fn macro_form_source_metered(
        &self, form: &Form, meter: &mut WorkMeter<'_>,
    ) -> Result<Option<MacroFormSource<'_>>, Diagnostic> {
        if let Some(pair) = &self.reader_forms {
            let mut pending = vec![(&pair.0, &pair.1)];
            while let Some((lowered, reader)) = pending.pop() {
                charge(meter, 1, &form.span)?;
                if same_metered(lowered, form, true, &mut *meter)? {
                    return Ok(Some(MacroFormSource::Borrowed(reader)));
                }
                // Metadata has an independent shape; source indexing may add
                // prefixes. Never let it shift correspondence of code children.
                charge(
                    meter,
                    lowered.metadata.len().min(reader.metadata.len()),
                    &form.span,
                )?;
                pending.extend(lowered.metadata.iter().zip(&reader.metadata));
                if std::mem::discriminant(&lowered.kind) == std::mem::discriminant(&reader.kind) {
                    let (a, b) = (code_children(lowered), code_children(reader));
                    if a.len() == b.len() {
                        charge(meter, a.len(), &form.span)?;
                        pending.extend(a.iter().zip(b));
                    }
                }
            }
        }
        self.original_form_ref_metered(form, meter)
    }

    pub(crate) fn with_reader_expansion(&self, lowered: Form, reader: Form) -> Self {
        let mut result = self.clone();
        result.reader_forms = Some(std::sync::Arc::new((lowered, reader)));
        result
    }

    /// Synthesized defmacro wrappers may not match source. Enrich only verified
    /// original subtrees and leave every compiler-generated location unknown.
    pub(crate) fn reader_tree(&self, form: &Form) -> Result<Form, Diagnostic> {
        let mut remaining = 1_048_576u64;
        self.reader_tree_metered(form, &mut |work| {
            remaining = remaining.checked_sub(work).ok_or_else(|| Diagnostic {
                span: form.span.clone(),
                message: "Source provenance computation fuel exhausted".into(),
            })?;
            Ok(())
        })
    }

    pub(crate) fn reader_tree_metered(
        &self,
        form: &Form,
        meter: &mut impl FnMut(u64) -> Result<(), Diagnostic>,
    ) -> Result<Form, Diagnostic> {
        let mut pending = vec![(form, false, 0usize)];
        let mut completed = Vec::<Form>::new();
        while let Some((node, finish, child_count)) = pending.pop() {
            if !finish {
                charge(meter, 1, &node.span)?;
                if let Some(original) = self.original_form_data_metered(node, &mut *meter)? {
                    completed.push(original);
                    continue;
                }
                let children = all_children(node);
                charge(meter, children.len(), &node.span)?;
                pending.push((node, true, children.len()));
                pending.extend(children.into_iter().rev().map(|child| (child, false, 0)));
            } else {
                let children = completed.split_off(completed.len() - child_count);
                completed.push(rebuild_form(node, children));
            }
        }
        Ok(completed
            .pop()
            .expect("reader tree enrichment produces a root"))
    }

    fn original_form_data_metered(
        &self, form: &Form, meter: &mut WorkMeter<'_>,
    ) -> Result<Option<Form>, Diagnostic> {
        self.original_form_ref_metered(form, meter)?
            .map(|found| match found {
                MacroFormSource::Borrowed(form)=>clone_form_metered(form,meter),
                MacroFormSource::Original(node)=>demand_read(meter,|meter|node.located_metered(meter)),
            }).transpose()
    }

    fn original_form_ref_metered(
        &self,form:&Form,meter:&mut WorkMeter<'_>,
    )->Result<Option<MacroFormSource<'_>>,Diagnostic> {
        if self.metadata_forms.get().is_none() {
            let file=self.path.as_ref().map(|path| {
                    charge(meter,path.as_os_str().len(),&form.span)?;
                    path.to_str().map(str::to_owned).ok_or_else(||Diagnostic{span:form.span.clone(),message:"Source metadata path is not Unicode".into()})
                }).transpose()?;
            let result=(|| -> Result<MetadataSnapshot,Diagnostic> {
                charge(meter,2,&form.span)?;
                let source=demand_read(meter,|meter|suss_reader::forms::SourceMetadata::read_metered(self.text.clone(),file,meter)).map_err(|error|Diagnostic{span:form.span.clone(),message:format!("Unable to construct source metadata snapshot: {}",error.message)})?;
                // Keep the old second read (including anonymous reader IDs) for
                // unsupported mappings, before original index construction.
                let located_forms=if source.requires_eager() {
                    let path=self.path.as_ref().and_then(|p|p.to_str());
                    Some(demand_read(meter,|meter|suss_reader::forms::read_forms_with_source_metadata_metered(&self.text,path,meter)).map_err(|error|Diagnostic{span:form.span.clone(),message:format!("Unable to construct source metadata snapshot: {}",error.message)})?)
                } else {None};
                let index=FormIndex::new_metered(source.forms(),meter)?;
                let located=std::sync::OnceLock::new();
                if let Some(forms)=located_forms {let _=located.set(IndexedForms::new_metered(forms,meter)?);}
                if std::env::var_os("SUSS_METADATA_FUEL_TRACE").is_some() {eprintln!("origin-demand snapshot bytes={} eager={}",self.text.len(),source.requires_eager());}
                Ok(MetadataSnapshot {source,index,located})
            })()?;
            let _=self.metadata_forms.set(Ok(result));
        }
        let snapshot=self.metadata_forms.get().expect("snapshot initialized").as_ref().map_err(|error|Diagnostic{
            span:form.span.clone(),message:format!("Unable to construct source metadata snapshot: {error}"),
        })?;
        let Some(node)=snapshot.index.find_source_metered(&snapshot.source,form,meter)? else {return Ok(None)};
        if node.is_caret_prefix() && snapshot.located.get().is_none() {
            // Legacy span-index selection can see generated metadata maps at
            // these same spans. Preserve that exact behavior through fallback.
            let path=self.path.as_ref().and_then(|p|p.to_str());
            let forms=demand_read(meter,|meter|suss_reader::forms::read_forms_with_source_metadata_metered(&self.text,path,meter))?;
            let located=IndexedForms::new_metered(forms,meter)?;
            let _=snapshot.located.set(located);
            if std::env::var_os("SUSS_METADATA_FUEL_TRACE").is_some() {eprintln!("origin-demand late-eager reason=caret-prefix bytes={}",self.text.len());}
        }
        if let Some(located)=snapshot.located.get() {
            let Some(found)=located.find_metered(form,false,meter)? else {return Err(Diagnostic {span:form.span.clone(),message:"Verified source form missing from source metadata snapshot".into()})};
            if std::env::var_os("SUSS_METADATA_FUEL_TRACE").is_some() {eprintln!("origin-demand mode=eager-return bytes={}",self.text.len());}
            return Ok(Some(MacroFormSource::Borrowed(found)));
        }
        Ok(Some(MacroFormSource::Original(node)))
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atomic_and_reused_container_clones_preserve_values_and_error_boundaries() {
        for source in ["nil", "true", "42", "\"A😀\"", "plain", "ns/name", ":plain", ":ns/name",
            "[]", "[a 1 :k]", "(a b)", "{:a 1}", "#{:a :b}", "'(a b)", "^:tag [1 2]", "#?(:cljs [1 2])"]
        {
            let form = read_forms(source).unwrap().remove(0);
            let expected = form.clone();
            let mut charges = Vec::new();
            let cloned = clone_form_metered(&form, &mut |work| { charges.push(work); Ok(()) }).unwrap();
            assert_eq!(cloned, expected, "{source}");
            assert_eq!(cloned.span, expected.span);
            assert_eq!(cloned.metadata, expected.metadata);
            if source == "nil" { assert_eq!(charges, [1], "atomic nil has no scratch storage"); }
            for stop in 0..charges.len() {
                let mut call = 0;
                let error = clone_form_metered(&form, &mut |_| {
                    let current = call; call += 1;
                    if current == stop { Err(Diagnostic { span: 0..0, message: "clone budget/interrupt".into() }) } else { Ok(()) }
                }).unwrap_err();
                assert_eq!(error.message, "clone budget/interrupt");
                assert_eq!(form, expected, "failed clone changed source {source}");
            }
        }
    }

    #[test]
    fn reconstruction_retains_final_metadata_and_code_allocations() {
        for source in ["[a 1 :k]", "^:outer ^{:inner 1} [a 1 :k]"] {
            let form = read_forms(source).unwrap().remove(0);
            let Kind::Vector(children) = &form.kind else { unreachable!() };
            let mut frame = new_clone_frame(&form, &mut |_| Ok(())).unwrap();
            let metadata_allocation = frame.metadata.as_ptr();
            let values_allocation = frame.values.as_ptr();
            for metadata in &form.metadata { frame.metadata.push(metadata.clone()); }
            for child in children { frame.values.push(child.clone()); }
            let rebuilt = finish_cloned_form(&form, frame.metadata, frame.values,
                &mut |_| panic!("final containers already own their backing and children")).unwrap();
            assert_eq!(rebuilt, form);
            assert_eq!(rebuilt.metadata.as_ptr(), metadata_allocation);
            let Kind::Vector(values) = &rebuilt.kind else { unreachable!() };
            assert_eq!(values.as_ptr(), values_allocation);
        }
    }

    #[test]
    fn streaming_clone_preserves_deep_wide_and_reader_only_children() {
        let leaf = read_forms("leaf").unwrap().remove(0);
        let operator = read_forms("quote").unwrap().remove(0);
        let metadata = read_forms(":tag").unwrap().remove(0);
        let mut deep = leaf.clone();
        for depth in 0..128 {
            deep = Form { span: depth..512, metadata: vec![metadata.clone()],
                kind: Kind::Prefix { operator: Box::new(operator.clone()), target: Box::new(deep) } };
        }
        let discarded = Form { span: 5..19, metadata: vec![metadata.clone()], kind: Kind::Discard(Box::new(leaf.clone())) };
        let conditional = Form { span: 4..20, metadata: vec![metadata.clone()],
            kind: Kind::Conditional(vec![metadata.clone(), discarded]) };
        let splice = Form { span: 3..21, metadata: vec![], kind: Kind::SplicingConditional(vec![metadata.clone(), conditional]) };
        let wide = Form { span: 0..512, metadata: vec![metadata], kind: Kind::Vector(vec![leaf; 256]) };
        for form in [deep, splice, wide] {
            let expected = form.clone();
            let mut calls = 0;
            assert_eq!(clone_form_metered(&form, &mut |_| { calls += 1; Ok(()) }).unwrap(), expected);
            for stop in [0, calls / 2, calls - 1] {
                let mut call = 0;
                assert!(clone_form_metered(&form, &mut |_| {
                    let current = call; call += 1;
                    if current == stop { Err(Diagnostic { span: 0..0, message: "stream interrupted".into() }) } else { Ok(()) }
                }).is_err());
                assert_eq!(form, expected);
            }
        }
    }

    #[test]
    fn metered_clone_preserves_compound_forms_and_checks_every_allocation_boundary() {
        let form = read_forms("^{:audit [1 2 3]} [foo :bar \"text\" '(a b) #?(:cljs [4 5])]").unwrap().remove(0);
        let mut charges = Vec::new();
        let cloned = clone_form_metered(&form, &mut |units| { charges.push(units); Ok(()) }).unwrap();
        assert_eq!(cloned, form);
        assert_eq!(cloned.span, form.span);
        assert_eq!(cloned.metadata, form.metadata);
        for at in 0..charges.len() {
            let mut count = 0;
            let error = clone_form_metered(&form, &mut |_| {
                let current = count; count += 1;
                if current == at { Err(Diagnostic { span: 0..0, message: "clone interrupted".into() }) } else { Ok(()) }
            }).unwrap_err();
            assert_eq!(error.message, "clone interrupted");
            assert_eq!(clone_form_metered(&form, &mut |_| Ok(())).unwrap(), form);
        }
        let mut impossible = Vec::<u8>::new();
        assert!(clone_reserve(&mut impossible, usize::MAX, &mut |_| Ok(()), &form.span).is_err());
        assert!(impossible.is_empty());
        let mut saw = None;
        assert!(new_clone_frame(&form, &mut |units| {
            saw = Some(units);
            Err(Diagnostic { span: 0..0, message: "before child storage".into() })
        }).is_err());
        assert!(saw.unwrap() > 0);
    }

    #[test]
    fn demand_caret_prefix_fallback_preserves_legacy_generated_span_selection() {
        for source in ["^number (f)","^:outer (f)","^{:line 7} (f)","^\"tag\" (f)"] {
            let original=read_forms(source).unwrap().remove(0);
            let wanted=&original.metadata[0];
            let located=IndexedForms::new(read_forms_with_source_metadata(source,None).unwrap());
            let expected=located.find_metered(wanted,false,&mut |_|Ok(())).unwrap();
            let origin=SourceOrigin::new(source,None);
            let actual=origin.macro_form_data_metered(wanted,&mut |_|Ok(()));
            match expected {Some(expected)=>assert_eq!(&actual.unwrap(),expected,"{source}"),None=>assert!(actual.is_err(),"{source}")}
            assert!(origin.metadata_forms.get().unwrap().as_ref().unwrap().located.get().is_some());
        }
    }
    #[test]
    fn demand_origin_symbol_then_root_then_full_composite_never_repays_eager_forest() {
        let source=format!("(def ^:dynamic target [{}])",vec!["17";2048].join(" "));
        let original=read_forms(&source).unwrap().remove(0);
        let Kind::List(items)=&original.kind else {unreachable!()};
        let origin=SourceOrigin::new(source.clone(),Some("demand.sus".into()));
        let mut cold=0;
        origin.macro_form_data_metered(&items[1],&mut |n|{cold+=n;Ok(())}).unwrap();
        assert!(origin.metadata_forms.get().unwrap().as_ref().unwrap().located.get().is_none());
        let mut root_cost=0;
        origin.macro_form_root_metadata_metered(&original,&mut |n|{root_cost+=n;Ok(())}).unwrap();
        let mut full_cost=0;
        let full=origin.macro_form_data_metered(&original,&mut |n|{full_cost+=n;Ok(())}).unwrap();
        assert_eq!(full,read_forms_with_source_metadata(&source,Some("demand.sus")).unwrap().remove(0));
        assert!(origin.metadata_forms.get().unwrap().as_ref().unwrap().located.get().is_none());
        // Execute the previous independent parser/index/clone route, using
        // genuine readers and matching rather than a mocked validity result.
        let mut legacy_cost=0;
        let mut legacy_meter=|n|{legacy_cost+=n;Ok(())};
        let raw=demand_read(&mut legacy_meter,|meter|suss_reader::forms::read_forms_metered(&source,meter)).unwrap();
        let raw=demand_read(&mut legacy_meter,|meter|suss_reader::forms::resolve_conditionals_metered(raw,meter)).unwrap();
        let located=demand_read(&mut legacy_meter,|meter|suss_reader::forms::read_forms_with_source_metadata_metered(&source,Some("demand.sus"),meter)).unwrap();
        let raw=IndexedForms::new_metered(raw,&mut legacy_meter).unwrap();
        let located=IndexedForms::new_metered(located,&mut legacy_meter).unwrap();
        for (wanted,root_only) in [(&items[1],false),(&original,true),(&original,false)] {
            assert!(raw.find_metered(wanted,true,&mut legacy_meter).unwrap().is_some());
            let selected=located.find_metered(wanted,false,&mut legacy_meter).unwrap().unwrap();
            if root_only {
                charge(&mut legacy_meter,selected.metadata.len()*std::mem::size_of::<Form>(),&selected.span).unwrap();
                for prefix in &selected.metadata {
                    charge(&mut legacy_meter,1,&selected.span).unwrap();
                    clone_form_metered(prefix,&mut legacy_meter).unwrap();
                }
            } else {clone_form_metered(selected,&mut legacy_meter).unwrap();}
        }
        let total=cold+root_cost+full_cost;
        assert!(total<legacy_cost,"demand={total} legacy={legacy_cost}");
        eprintln!("demand-origin cold-symbol={cold} root={root_cost} later-full={full_cost} total={total} legacy-total={legacy_cost} eager=false");
    }
    #[test]
    fn demand_origin_rejects_changed_bits_namespace_metadata_and_forged_string_span() {
        let source="^{:kept 1} [0 -0.0 foo/bar]";
        let original=read_forms(source).unwrap().remove(0);
        let origin=SourceOrigin::new(source,None);
        origin.macro_form_data_metered(&original,&mut |_|Ok(())).unwrap();
        for edit in 0..3 {
            let mut changed=original.clone();
            match edit {
                0=>{let Kind::Vector(v)=&mut changed.kind else {unreachable!()};v[1].kind=Kind::Number(0.0);}
                1=>{let Kind::Vector(v)=&mut changed.kind else {unreachable!()};v[2].kind=Kind::Symbol(suss_reader::Symbol::namespaced("other","bar"));}
                _=>changed.metadata=vec![read_forms(":changed").unwrap().remove(0)],
            }
            assert_eq!(origin.macro_form_data_metered(&changed,&mut |_|Ok(())).unwrap(),changed);
        }
        let nan_origin=SourceOrigin::new("##NaN",None);
        let mut nan=read_forms("##NaN").unwrap().remove(0);
        let payload=0x7ff8_0000_0000_0001;
        nan.kind=Kind::Number(f64::from_bits(payload));
        let result=nan_origin.macro_form_data_metered(&nan,&mut |_|Ok(())).unwrap();
        let Kind::Number(number)=result.kind else {unreachable!()};
        assert_eq!(number.to_bits(),payload);
        let mut forged=read_forms("target").unwrap().remove(0);forged.span=1..7;
        assert_eq!(SourceOrigin::new("\"target\"",None).macro_form_data_metered(&forged,&mut |_|Ok(())).unwrap(),forged);
    }
    #[test]
    fn root_metadata_projection_preserves_values_with_less_work_than_full_copy() {
        let source = format!("(def target [{}])", vec!["17"; 512].join(" "));
        let form = read_forms(&source).unwrap().remove(0);
        let origin = SourceOrigin::new(source, None);
        let expected = origin.macro_form_data_metered(&form, &mut |_| Ok(())).unwrap();
        let mut full_work = 0;
        let full = origin.macro_form_data_metered(&form, &mut |n| {full_work += n; Ok(())}).unwrap();
        let mut root_work = 0;
        let (span, metadata) = origin.macro_form_root_metadata_metered(&form, &mut |n| {root_work += n; Ok(())}).unwrap();
        assert_eq!(span, full.span);
        assert_eq!(metadata, expected.metadata);
        assert_eq!(metadata, full.metadata);
        assert!(root_work < full_work, "root-only work {root_work} must avoid full copy {full_work}");
    }

    #[test]
    fn root_metadata_projection_preserves_reader_correspondence_and_unverified_fallback() {
        let source = "^{:line 99 :custom :kept} (def target [1 2])";
        let form = read_forms(source).unwrap().remove(0);
        let origin = SourceOrigin::new(source, None);
        let full = origin.macro_form_data_metered(&form, &mut |_| Ok(())).unwrap();
        let mut reader = full.clone();
        reader.span = 100..200;
        let expanded = SourceOrigin::new("", None).with_reader_expansion(form.clone(), reader.clone());
        let (span, metadata) = expanded.macro_form_root_metadata_metered(&form, &mut |_| Ok(())).unwrap();
        assert_eq!(span, reader.span);
        assert_eq!(metadata, reader.metadata);
        let mut changed = form.clone();
        changed.kind = Kind::List(vec![]);
        assert_eq!(origin.macro_form_root_metadata_metered(&changed, &mut |_| Ok(())).unwrap().1,changed.metadata);
        let mut forged = read_forms("target").unwrap().remove(0);
        forged.span = 1..7;
        let string_origin = SourceOrigin::new("\"target\"", None);
        assert!(string_origin.macro_form_root_metadata_metered(&forged, &mut |_| Ok(())).unwrap().1.is_empty());
    }

    #[test]
    fn root_metadata_projection_charges_cold_and_warm_cancellation_and_recovers() {
        let source = format!("(def target [{}])", vec!["17"; 512].join(" "));
        let form = read_forms(&source).unwrap().remove(0);
        let origin = SourceOrigin::new(source, None);
        let mut work = 0;
        let error = origin.macro_form_root_metadata_metered(&form, &mut |n| {
            work += n;
            if work > 128 {Err(Diagnostic {span:0..1,message:"projection cancelled".into()})} else {Ok(())}
        }).unwrap_err();
        assert_eq!(error.message,"Unable to construct source metadata snapshot: projection cancelled");
        assert!(origin.metadata_forms.get().is_none());
        let metadata = origin.macro_form_root_metadata_metered(&form, &mut |_| Ok(())).unwrap();
        let expected = origin.macro_form_data_metered(&form, &mut |_| Ok(())).unwrap();
        assert_eq!(metadata.1,expected.metadata);
        assert!(origin.macro_form_root_metadata_metered(&form, &mut |_| Err(Diagnostic {span:0..1,message:"warm cancelled".into()})).is_err());
        assert_eq!(origin.macro_form_root_metadata_metered(&form, &mut |_| Ok(())).unwrap(),metadata);
    }

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
    fn provenance_snapshot_metering_charges_cold_build_and_warm_lookups() {
        let source = "^:marked (identity [false nil -0.0]) {:key \"value\"}";
        let original = resolve_conditionals(read_forms(source).unwrap()).unwrap();
        let located = read_forms_with_source_metadata(source, Some("snapshot.sus")).unwrap();
        let origin = SourceOrigin::new(source, Some("snapshot.sus".into()));
        let clone = origin.clone();
        let mut cold_work = 0u64;
        let cold = clone
            .original_form_data_metered(original.first().unwrap(), &mut |amount| {
                cold_work += amount;
                Ok(())
            })
            .unwrap();
        let mut warm_work = 0u64;
        let warm = origin
            .original_form_data_metered(original.first().unwrap(), &mut |amount| {
                warm_work += amount;
                Ok(())
            })
            .unwrap();
        assert!(
            cold.as_ref()
                .is_some_and(|form| same(form, located.first().unwrap(), false))
        );
        assert!(
            warm.as_ref()
                .is_some_and(|form| same(form, located.first().unwrap(), false))
        );
        assert!(
            cold_work > warm_work,
            "cold snapshot construction must be charged"
        );
        assert!(
            warm_work > 0,
            "warm cache lookup/comparison/clone must still be charged"
        );

        let mut remaining = 0u64;
        let failure = origin
            .original_form_data_metered(original.first().unwrap(), &mut |amount| {
                remaining = remaining.checked_add(amount).ok_or_else(|| Diagnostic {
                    span: 0..0,
                    message: "test counter overflow".into(),
                })?;
                Err(Diagnostic {
                    span: 0..0,
                    message: "host budget exhausted".into(),
                })
            })
            .unwrap_err();
        assert_eq!(failure.span, original[0].span);
        assert_eq!(failure.message, "host budget exhausted");
        assert!(remaining > 0);
        assert!(std::sync::Arc::ptr_eq(
            &origin.metadata_forms,
            &clone.metadata_forms
        ));
        assert!(origin.metadata_forms.get().unwrap().is_ok());
    }

    #[test]
    fn metered_reader_provenance_handles_deep_forms_without_recursive_comparison_or_clone() {
        fn nested(depth: usize, leaf: Form) -> Form {
            (0..depth).fold(leaf, |form, _| Form {
                span: 0..1,
                metadata: vec![],
                kind: Kind::Vector(vec![form]),
            })
        }
        let leaf = Form {
            span: 0..1,
            metadata: vec![],
            kind: Kind::Number(-0.0),
        };
        let lowered = nested(96, leaf.clone());
        let reader = nested(96, leaf);
        let origin =
            SourceOrigin::new("", None).with_reader_expansion(lowered.clone(), reader.clone());
        let mut work = 0u64;
        let observed = origin
            .macro_form_data_metered(&lowered, &mut |amount| {
                work += amount;
                Ok(())
            })
            .unwrap();
        assert_eq!(observed, reader);
        assert!(work > 96);
        let different = nested(
            96,
            Form {
                span: 0..1,
                metadata: vec![],
                kind: Kind::Number(0.0),
            },
        );
        let mut mismatch_work = 0u64;
        let observed = origin
            .macro_form_data_metered(&different, &mut |amount| {
                mismatch_work += amount;
                Ok(())
            })
            .unwrap();
        assert_eq!(
            observed, different,
            "binary64 sign bits remain part of provenance identity"
        );
        assert!(mismatch_work > 96);
    }

    #[test]
    fn metered_provenance_accepts_source_larger_than_legacy_text_cap() {
        let source = format!("{}target", " ".repeat(1_100_000));
        let form = read_forms(&source).unwrap().remove(0);
        let origin = SourceOrigin::new(source, None);
        let mut work = 0u64;
        let observed = origin
            .macro_form_data_metered(&form, &mut |amount| {
                work += amount;
                Ok(())
            })
            .unwrap();
        assert_eq!(observed.span, form.span);
        assert_eq!(observed.kind, form.kind);
        assert!(work > 2_200_000);
    }
}
