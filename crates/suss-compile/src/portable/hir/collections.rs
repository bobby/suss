//! Original literal lowering following the pinned compiler's constructor paths.
//! Reference: cljs/compiler.cljc emit-map/vector/set, c4295f30 (upstream EPL-1.0).
//! No persistent collection implementation or upstream source is copied here.
use super::*;

impl Analyzer<'_> {
    fn collection_class(&mut self, form: &Form, name: &str) -> Result<Hir, Diagnostic> {
        let symbol = suss_reader::Symbol {
            namespace: Some("suss.core".into()),
            name: name.into(),
        };
        let (kind, ty) = self.global_value(&symbol, form.span.clone())?;
        Ok(Hir {
            source: None,
            span: form.span.clone(),
            metadata: vec![],
            ty,
            kind,
        })
    }

    fn collection_property(
        &mut self,
        form: &Form,
        class: &str,
        property: &str,
    ) -> Result<Hir, Diagnostic> {
        let owner = self.collection_class(form, class)?;
        let key = self.literal_form(form, Literal::String(property.encode_utf16().collect()));
        Ok(self.nominal(form, Nominal::NamedGet, vec![owner, key]))
    }

    fn collection_array(&self, form: &Form, arguments: Vec<Hir>) -> Hir {
        Hir {
            source: None,
            span: form.span.clone(),
            metadata: vec![],
            ty: Type::Value,
            kind: Expression::Array {
                operation: ArrayOperation::Literal,
                arguments,
            },
        }
    }

    fn collection_method(
        &mut self,
        form: &Form,
        class: &str,
        property: &str,
        arguments: Vec<Hir>,
    ) -> Result<Hir, Diagnostic> {
        self.collection_method_with_entries(form, class, property, arguments, vec![])
    }

    fn collection_method_with_entries(
        &mut self,
        form: &Form,
        class: &str,
        property: &str,
        arguments: Vec<Hir>,
        entries: Vec<Binding>,
    ) -> Result<Hir, Diagnostic> {
        // Match member-call ordering: owner and method lookup precede all entries.
        let owner = self.collection_class(form, class)?;
        let owner_binding = self.fresh_binding(form, owner);
        let owner_read = self.local(form, owner_binding.id);
        let key = self.literal_form(form, Literal::String(property.encode_utf16().collect()));
        let lookup = self.nominal(form, Nominal::NamedGet, vec![owner_read.clone(), key]);
        let method_binding = self.fresh_binding(form, lookup);
        let mut operands = vec![self.local(form, method_binding.id), owner_read];
        operands.extend(arguments);
        let body = self.nominal(form, Nominal::ObjectInvoke, operands);
        let mut bindings = vec![owner_binding, method_binding];
        bindings.extend(entries);
        Ok(Hir {
            source: None,
            kind: Expression::Let {
                bindings,
                body: Box::new(body),
            },
            ..self.nominal(form, Nominal::ObjectInvoke, vec![])
        })
    }

    pub(super) fn vector_literal(
        &mut self,
        form: &Form,
        items: &[Form],
    ) -> Result<Hir, Diagnostic> {
        let entries = items
            .iter()
            .map(|item| self.form(item))
            .collect::<Result<Vec<_>, _>>()?;
        *self.source_nodes.last_mut().expect("source node fact slot") =
            Some(std::sync::Arc::new(SourceNode::Vector(entries.clone().into())));
        self.vector_values(form, entries)
    }

    pub(super) fn vector_values(
        &mut self,
        form: &Form,
        entries: Vec<Hir>,
    ) -> Result<Hir, Diagnostic> {
        if entries.is_empty() {
            return self.collection_property(form, "PersistentVector", "EMPTY");
        }
        let constructor = self.collection_class(form, "PersistentVector")?;
        let count = entries.len();
        let array = self.collection_array(form, entries);
        if count >= 32 {
            let no_clone = self.literal_form(form, Literal::Bool(true));
            return self.collection_method(
                form,
                "PersistentVector",
                "fromArray",
                vec![array, no_clone],
            );
        }
        let nil = self.literal_form(form, Literal::Nil);
        let count = self.literal_form(form, Literal::Number(count as f64));
        let shift = self.literal_form(form, Literal::Number(5.0));
        let root = self.collection_property(form, "PersistentVector", "EMPTY_NODE")?;
        Ok(self.nominal(
            form,
            Nominal::Construct,
            vec![constructor, nil.clone(), count, shift, root, array, nil],
        ))
    }

    pub(super) fn map_literal(&mut self, form: &Form, items: &[Form]) -> Result<Hir, Diagnostic> {
        if items.len() % 2 != 0 {
            return Err(fail(
                form.span.clone(),
                "Map literal requires paired entries",
            ));
        }
        let entries = items
            .iter()
            .map(|item| self.form(item))
            .collect::<Result<Vec<_>, _>>()?;
        *self.source_nodes.last_mut().expect("source node fact slot") =
            Some(std::sync::Arc::new(SourceNode::Map(entries.clone().into())));
        self.map_values(form, items, entries)
    }

    pub(super) fn map_values(
        &mut self,
        form: &Form,
        items: &[Form],
        values: Vec<Hir>,
    ) -> Result<Hir, Diagnostic> {
        if items.is_empty() {
            return self.collection_property(form, "PersistentArrayMap", "EMPTY");
        }
        if items.len() > 16 {
            // Preserve textual key/value order before splitting storage arrays.
            // The accepted 2026-10-01 decision records the pinned emitter variance.
            let mut entries = Vec::new();
            let mut keys = Vec::new();
            let mut lowered = values.into_iter();
            let mut values = Vec::new();
            for entry in items.chunks_exact(2) {
                let key = lowered.next().expect("paired map key");
                let key = self.fresh_binding(&entry[0], key);
                keys.push(self.local(&entry[0], key.id));
                entries.push(key);
                let value = lowered.next().expect("paired map value");
                let value = self.fresh_binding(&entry[1], value);
                values.push(self.local(&entry[1], value.id));
                entries.push(value);
            }
            let keys = self.collection_array(form, keys);
            let values = self.collection_array(form, values);
            return self.collection_method_with_entries(
                form,
                "PersistentHashMap",
                "fromArrays",
                vec![keys, values],
                entries,
            );
        }
        let constructor = self.collection_class(form, "PersistentArrayMap")?;
        let array = self.collection_array(form, values);
        if !distinct_constants(&items.iter().step_by(2).collect::<Vec<_>>()) {
            return self.collection_method(
                form,
                "PersistentArrayMap",
                "createAsIfByAssoc",
                vec![array],
            );
        }
        let nil = self.literal_form(form, Literal::Nil);
        let count = self.literal_form(form, Literal::Number((items.len() / 2) as f64));
        Ok(self.nominal(
            form,
            Nominal::Construct,
            vec![constructor, nil.clone(), count, array, nil],
        ))
    }

    pub(super) fn set_literal(&mut self, form: &Form, items: &[Form]) -> Result<Hir, Diagnostic> {
        let entries = items.iter().map(|item| self.form(item)).collect::<Result<Vec<_>, _>>()?;
        *self.source_nodes.last_mut().expect("source node fact slot") =
            Some(std::sync::Arc::new(SourceNode::Set(entries.clone().into())));
        self.set_values(form, items, entries)
    }

    /// Entries have already been analyzed as expressions or lowered as quoted
    /// data. Both paths use the same constructor/factory and runtime ordering.
    pub(super) fn set_values(&mut self, form: &Form, items: &[Form], values: Vec<Hir>) -> Result<Hir, Diagnostic> {
        if items.is_empty() {
            return self.collection_property(form, "PersistentHashSet", "EMPTY");
        }
        if items.len() > 8 || !distinct_constants(&items.iter().collect::<Vec<_>>()) {
            let array = self.collection_array(form, values);
            return self.collection_method(
                form,
                "PersistentHashSet",
                "createAsIfByAssoc",
                vec![array],
            );
        }
        let set_class = self.collection_class(form, "PersistentHashSet")?;
        let map_class = self.collection_class(form, "PersistentArrayMap")?;
        let nil = self.literal_form(form, Literal::Nil);
        let count = self.literal_form(form, Literal::Number(items.len() as f64));
        let mut entries = Vec::new();
        for value in values {
            entries.push(value);
            entries.push(nil.clone());
        }
        let array = self.collection_array(form, entries);
        let map = self.nominal(
            form,
            Nominal::Construct,
            vec![map_class, nil.clone(), count, array, nil.clone()],
        );
        Ok(self.nominal(
            form,
            Nominal::Construct,
            vec![set_class, nil.clone(), map, nil],
        ))
    }
}

fn distinct_constants(items: &[&Form]) -> bool {
    items.iter().enumerate().all(|(index, item)| {
        let constant = matches!(
            item.kind,
            Kind::Nil | Kind::Bool(_) | Kind::Number(_) | Kind::String(_)
        );
        constant
            && items[..index]
                .iter()
                .all(|other| !same_constant(item, other))
    })
}
fn same_constant(a: &Form, b: &Form) -> bool {
    match (&a.kind, &b.kind) {
        (Kind::Nil, Kind::Nil) => true,
        (Kind::Bool(a), Kind::Bool(b)) => a == b,
        (Kind::Number(a), Kind::Number(b)) => a == b,
        (Kind::String(a), Kind::String(b)) => a == b,
        _ => false,
    }
}
