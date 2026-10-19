//! Per-Graph native source-owner projections. Cache identity is retained owner
//! identity, never a source-key/memo/content equivalence assertion.
use super::*;
use portable::resolve::{CompletedDeclarationSource, VALUE_FIELDS};
type Info = Arc<portable::resolve::DefinitionInfo>;
type Fields = Vec<(&'static str, Value)>;

impl AnalysisGraph<'_> {
    pub(super) fn completed_owner_is_current(&mut self, global: &portable::resolve::Global,
        owner: &Arc<CompletedDeclarationSource>, holder: &Info) -> Result<bool> {
        self.charge(global.namespace().len().saturating_add(global.name().len()).saturating_mul(2).saturating_add(8))?;
        if !owner.belongs_to(global) { return Ok(false); }
        preserve_source_error(holder.declaration.span.clone(), &mut |n| self.session.charge_host_work(n),
            |meter| owner.matches_ordinary(holder, meter))
    }
    pub(super) fn completed_owner_binding(&mut self, global: &portable::resolve::Global,
        owner: &Arc<CompletedDeclarationSource>, holder: &Info) -> Result<()> {
        self.charge(global.namespace().len().saturating_add(global.name().len()).saturating_mul(6).saturating_add(40))?;
        let held = holder.completed_values.as_ref().is_some_and(|x| Arc::ptr_eq(x, owner))
            || holder.type_key_history.as_ref().and_then(|h| h.values(global))
                .and_then(|v| v.source.as_ref()).is_some_and(|x| Arc::ptr_eq(x, owner));
        let ordinary_current = if holder.type_fields.is_none() {
            self.completed_owner_is_current(global, owner, holder)?
        } else if let Some(plan) = holder.type_key_history.as_ref().and_then(|h| h.values(global)) {
            preserve_source_error(holder.declaration.span.clone(), &mut |n| self.session.charge_host_work(n),
                |meter| plan.matches_current_type(holder, meter))?
        } else { false };
        if !owner.belongs_to(global) || !held || !ordinary_current {
            return Err(SessionError::Host(wasmtime::Error::msg("Completed source owner binding mismatch")));
        }
        Ok(())
    }
    fn completed_lookup(&mut self, global: &portable::resolve::Global,
        owner: &Arc<CompletedDeclarationSource>, holder: &Info, kind: CompletedCacheKind) -> Result<Option<Value>> {
        self.completed_owner_binding(global, owner, holder)?;
        self.charge(global.namespace().len().saturating_add(global.name().len()).saturating_mul(2)
            .saturating_add(std::mem::size_of::<DeclarationCacheKey>() * 2 + 16))?;
        // Typed cache variants distinguish revisions, selected key occurrences,
        // completed values and source-value owners without pointer tagging.
        let key = DeclarationCacheKey::Completed(global.clone(), Arc::as_ptr(owner) as usize, kind);
        Ok(self.declarations.get(&key).map(|(_, v)| *v))
    }
    pub(super) fn completed_store(&mut self, global: &portable::resolve::Global,
        owner: &Arc<CompletedDeclarationSource>, holder: &Info, kind: CompletedCacheKind, value: Value) -> Result<()> {
        self.completed_owner_binding(global, owner, holder)?;
        self.charge(global.namespace().len().saturating_add(global.name().len()).saturating_mul(2)
            .saturating_add(std::mem::size_of::<(DeclarationCacheKey, (Option<Info>, Value))>() * 2 + 24))?;
        // The actual holder retains THIS exact owner through its native field
        // or collapsed plan, preventing pointer recycling for the cache lifetime.
        self.declarations.insert(DeclarationCacheKey::Completed(global.clone(), Arc::as_ptr(owner) as usize, kind), (Some(holder.clone()), value));
        Ok(())
    }
    pub(super) fn completed_pairs(&mut self, mut root: Value) -> Result<Vec<(Value, Value)>> {
        self.charge(4)?;
        for _ in 0..self.recipes.len() {
            self.charge(4)?;
            match self.recipes.get(root).and_then(Option::as_ref) {
                Some(Recipe::Alias(next)) => root = *next,
                Some(Recipe::Map(entries) | Recipe::ReaderMap(entries)) => {
                    let count = entries.len();
                    self.charge(std::mem::size_of::<Vec<(Value, Value)>>() * 2 + count * (std::mem::size_of::<(Value, Value)>() * 2 + 4) + 8)?;
                    let Some(Recipe::Map(entries) | Recipe::ReaderMap(entries)) = self.recipes[root].as_ref() else { unreachable!() };
                    return Ok(entries.clone());
                }
                _ => return Err(SessionError::Host(wasmtime::Error::msg("Invalid completed native map recipe"))),
            }
        }
        Err(SessionError::Host(wasmtime::Error::msg("Cyclic completed native map recipe")))
    }
    fn completed_reader(&mut self, owner: &CompletedDeclarationSource) -> Result<hir::ReaderMetadata> {
        validate_form_data(&owner.definition, &mut |units| self.session.charge_host_work(units))?;
        validate_form_data(&owner.declaration, &mut |units| self.session.charge_host_work(units))?;
        self.charge(6)?;
        let declaration = if let Some(origin) = &owner.origin {
            origin.macro_form_data_metered(&owner.declaration, &mut |units|
                charge_origin_work(self.session, owner.declaration.span.clone(), units)).map_err(SessionError::Compile)?
        } else { self.clone_form(&owner.declaration)? };
        crate::portable_macro_data::normalized_reader_metadata_view(self.session, &declaration)
    }
    fn completed_key_registry(&mut self, global: &portable::resolve::Global,
        owner: &Arc<CompletedDeclarationSource>, holder: &Info) -> Result<Value> {
        if let Some(root) = self.completed_lookup(global, owner, holder, CompletedCacheKind::KeyOccurrences)? { return Ok(root); }
        // The selected immutable reader is the actual clone input. Its full
        // normalized view owns the exact retained prefix/pair selectors; no
        // independently cloned output keys are equality-joined.
        let metadata = self.completed_reader(owner)?;
        self.charge(8)?;
        if metadata.pairs.len() / 2 != metadata.origins.len() || metadata.pairs.len() % 2 != 0
            || metadata.key_positions.len() != metadata.origins.len() {
            return Err(SessionError::Host(wasmtime::Error::msg("Completed metadata extent mismatch")));
        }
        self.charge(std::mem::size_of::<Vec<(Value, Value)>>() * 2 + 4)?;
        let mut keys = Vec::new();
        reuse::reserve(self.session, &mut keys, metadata.origins.len())?;
        for (index, pair) in metadata.pairs.chunks_exact(2).enumerate() {
            self.charge(8)?;
            let key = self.reader_key_recipe(&pair[0], metadata.origins[index])?;
            let ordinal = self.number(index)?;
            keys.push((key, ordinal));
        }
        let root = self.recipe(Recipe::Map(keys))?;
        self.completed_store(global, owner, holder, CompletedCacheKind::KeyOccurrences, root)?;
        Ok(root)
    }
    pub(super) fn completed_occurrence_key(&mut self, global: &portable::resolve::Global,
        owner: &Arc<CompletedDeclarationSource>, holder: &Info, label: &'static str) -> Result<Option<Value>> {
        let registry = self.completed_key_registry(global, owner, holder)?;
        let entries = self.completed_pairs(registry)?;
        Ok(self.declaration_field_position(&entries, label)?.map(|i| entries[i].0))
    }
    pub(super) fn completed_metadata_map(&mut self, global: &portable::resolve::Global,
        owner: &Arc<CompletedDeclarationSource>, holder: &Info, depth: usize) -> Result<Value> {
        if let Some(root) = self.completed_lookup(global, owner, holder, CompletedCacheKind::Metadata)? { return Ok(root); }
        let registry = self.completed_key_registry(global, owner, holder)?;
        let entries = self.completed_pairs(registry)?;
        let metadata = self.completed_reader(owner)?;
        self.charge(6)?;
        if entries.len() != metadata.origins.len() {
            return Err(SessionError::Host(wasmtime::Error::msg("Completed key registry extent mismatch")));
        }
        let mut keys = self.reader_key_slots(entries.len())?;
        for (index, (key, _)) in entries.iter().enumerate() { self.charge(4)?; keys[index] = Some(*key); }
        let value = self.declaration_reader_metadata_map(&metadata, &mut keys, global.namespace() == "suss.core", depth)?;
        // Alias is the existing immutable-record sharing barrier; ReaderMap
        // itself intentionally repeats. Only this genuine owner gets a barrier.
        let value = self.recipe(Recipe::Alias(value))?;
        self.completed_store(global, owner, holder, CompletedCacheKind::Metadata, value)?;
        Ok(value)
    }
    fn completed_set(&mut self, fields: &mut Fields, name: &'static str, value: Value) -> Result<()> {
        self.charge(4)?;
        self.charge(fields.len().saturating_mul(name.len().saturating_add(8)).saturating_add(4))?;
        fields.retain(|(label, _)| *label != name);
        reuse::reserve(self.session, fields, 1)?;
        self.charge(4)?;
        fields.push((name, value));
        Ok(())
    }
    fn completed_property<'b>(&mut self, metadata: &'b hir::ReaderMetadata, label: &str) -> Result<Option<&'b Form>> {
        for pair in metadata.pairs.chunks_exact(2) {
            self.charge(label.len().saturating_add(6))?;
            if matches!(&pair[0].kind, Kind::Keyword(k) if k.namespace.is_none() && k.name == label) { return Ok(Some(&pair[1])); }
        }
        Ok(None)
    }
    pub(super) fn completed_field_values(&mut self, global: &portable::resolve::Global,
        owner: &Arc<CompletedDeclarationSource>, holder: &Info, depth: usize) -> Result<Fields> {
        if let Some(root) = self.completed_lookup(global, owner, holder, CompletedCacheKind::Fields)? {
            let pairs = self.completed_pairs(root)?;
            self.charge(std::mem::size_of::<Fields>() * 2 + 4)?;
            let mut fields = Vec::new();
            reuse::reserve(self.session, &mut fields, VALUE_FIELDS.len())?;
            for label in VALUE_FIELDS {
                if let Some(value) = self.declaration_field_value(&pairs, label)? {
                    self.charge(4)?; fields.push((label, value));
                }
            }
            return Ok(fields);
        }
        // Same native ordinary value writers; only flat source data is retained.
        // No callable body/Info is recreated and no guest callback is executed.
        let metadata = self.completed_reader(owner)?;
        self.charge(std::mem::size_of::<Fields>() * 2 + 4)?;
        let mut fields = Vec::new();
        reuse::reserve(self.session, &mut fields, VALUE_FIELDS.len())?;
        let name = self.symbol(&format!("{}/{}", global.namespace(), global.name()))?;
        self.completed_set(&mut fields, "name", name)?;
        for label in VALUE_FIELDS {
            if matches!(label, "meta" | "line" | "column" | "file") { continue; }
            if let Some(value) = self.completed_property(&metadata, label)? {
                let value = self.form(value, depth + 1)?;
                self.completed_set(&mut fields, label, value)?;
            }
        }
        let truthy = |x: &Form| !matches!(x.kind, Kind::Nil | Kind::Bool(false));
        let declared = self.completed_property(&metadata, "declared")?.is_some_and(truthy);
        self.charge(4)?;
        let provisional = owner.has_initializer && declared;
        // A completed initializer-bearing :declared def publishes provisional
        // metadata, not the normal completed :meta/arity projection.
        if provisional {
            if let Some(value) = self.completed_property(&metadata, "meta")? {
                let value = self.form(value, depth + 1)?; self.completed_set(&mut fields, "meta", value)?;
            }
        } else {
            let value = self.completed_metadata_map(global, owner, holder, depth + 1)?;
            self.completed_set(&mut fields, "meta", value)?;
        }
        if let Some(origin) = &owner.origin {
            let (span, metadata) = origin.macro_form_root_metadata_metered(&owner.definition,
                &mut |work| charge_origin_work(self.session, owner.definition.span.clone(), work)).map_err(SessionError::Compile)?;
            self.charge(std::mem::size_of::<Form>() * 2 + 4)?;
            let position = crate::portable_macro_data::normalized_reader_metadata(self.session,
                &Form { span, metadata, kind: Kind::Nil })?;
            for label in ["line", "column", "file"] {
                for pair in position.chunks_exact(2) {
                    self.charge(label.len().saturating_add(6))?;
                    if matches!(&pair[0].kind, Kind::Keyword(k) if k.namespace.is_none() && k.name == label) {
                        let value = self.form(&pair[1], depth + 1)?;
                        self.completed_set(&mut fields, label, value)?; break;
                    }
                }
            }
        }
        if global.namespace() == "suss.core" {
            let value = self.scalar(Literal::String("cljs/core.cljs".encode_utf16().collect()))?;
            self.completed_set(&mut fields, "file", value)?;
        }
        if !provisional {
            if let Some(doc) = &owner.docstring {
                self.charge(doc.len().saturating_mul(4).saturating_add(8))?;
                let value = self.scalar(Literal::String(doc.clone()))?;
                self.completed_set(&mut fields, "doc", value)?;
            }
        }
        if let Some(function) = &owner.function {
            if !declared {
                let macro_flag = self.completed_property(&metadata, "macro")?.is_some_and(truthy);
                let value = self.flag(!macro_flag)?; self.completed_set(&mut fields, "fn-var", value)?;
                if let Some(top) = self.completed_property(&metadata, "top-fn")?.filter(|f| !matches!(f.kind, Kind::Nil)) {
                    self.charge(std::mem::size_of::<Vec<(&Form, &Form)>>() * 2 + 4)?;
                    let mut pairs = Vec::new();
                    match &top.kind {
                        Kind::Map(items) => {
                            reuse::reserve(self.session, &mut pairs, items.len() / 2)?;
                            for p in items.chunks_exact(2) { self.charge(4)?; pairs.push((&p[0], &p[1])); }
                        }
                        Kind::Vector(items) if items.len() == 2 => { reuse::reserve(self.session, &mut pairs, 1)?; self.charge(4)?; pairs.push((&items[0], &items[1])); }
                        Kind::List(x) | Kind::Set(x) if x.is_empty() => (),
                        Kind::String(x) if x.is_empty() => (),
                        _ => return Err(SessionError::Host(wasmtime::Error::msg("Function declaration top-fn requires mergeable reader data"))),
                    }
                    for label in VALUE_FIELDS {
                        if label == "export" || matches!(label, "type" | "num-fields" | "record") { continue; }
                        for (key, value) in pairs.iter().rev() {
                            self.charge(label.len().saturating_add(6))?;
                            if matches!(&key.kind, Kind::Keyword(k) if k.namespace.is_none() && k.name == label) {
                                let value = self.form(value, depth + 1)?;
                                self.completed_set(&mut fields, label, value)?; break;
                            }
                        }
                    }
                } else {
                    let value = self.flag(function.variadic)?; self.completed_set(&mut fields, "variadic?", value)?;
                    let value = self.number(function.fixed.ok_or_else(|| SessionError::Host(wasmtime::Error::msg("Invalid source function declaration")))?)?;
                    self.completed_set(&mut fields, "max-fixed-arity", value)?;
                    self.charge(std::mem::size_of::<Vec<Value>>() * 2 + 4)?;
                    let mut methods = Vec::new(); reuse::reserve(self.session, &mut methods, function.parameters.len())?;
                    for names in &function.parameters {
                        self.charge(std::mem::size_of::<Vec<Value>>() * 2 + 4)?;
                        let mut parameters = Vec::new(); reuse::reserve(self.session, &mut parameters, names.len())?;
                        for name in names { let value = self.symbol(name)?; self.charge(4)?; parameters.push(value); }
                        let value = self.vector(&parameters)?; self.charge(4)?; methods.push(value);
                    }
                    let value = self.recipe(Recipe::List(methods))?; self.completed_set(&mut fields, "method-params", value)?;
                    let arglists = self.completed_property(&metadata, "arglists")?;
                    let value = if let Some(x) = arglists { self.form(x, depth + 1)? } else { self.scalar(Literal::Nil)? };
                    self.completed_set(&mut fields, "arglists", value)?;
                    self.charge(std::mem::size_of::<Vec<Value>>() * 2 + 4)?;
                    let mut metas = Vec::new();
                    if let Some(arglists) = arglists {
                        match &arglists.kind {
                            Kind::List(items) | Kind::Vector(items) | Kind::Set(items) => {
                                reuse::reserve(self.session, &mut metas, items.len())?;
                                for item in items {
                                    let metadata = crate::portable_macro_data::normalized_reader_metadata(self.session, item)?;
                                    let value = if metadata.is_empty() { self.scalar(Literal::Nil)? }
                                        else { self.form(&Form { span: item.span.clone(), metadata: Vec::new(), kind: Kind::Map(metadata) }, depth + 1)? };
                                    self.charge(4)?; metas.push(value);
                                }
                            }
                            Kind::Map(items) => { reuse::reserve(self.session, &mut metas, items.len() / 2)?;
                                for _ in 0..items.len()/2 { let value = self.scalar(Literal::Nil)?; self.charge(4)?; metas.push(value); } }
                            Kind::String(units) => { reuse::reserve(self.session, &mut metas, units.len())?;
                                for _ in units { let value = self.scalar(Literal::Nil)?; self.charge(4)?; metas.push(value); } }
                            Kind::Nil => (),
                            _ => return Err(SessionError::Host(wasmtime::Error::msg("Function declaration arglists requires seqable reader data"))),
                        }
                    }
                    let value = self.recipe(Recipe::List(metas))?; self.completed_set(&mut fields, "arglists-meta", value)?;
                }
            }
        }
        if !provisional && declared {
            if let Some(arglists) = self.completed_property(&metadata, "arglists")?.filter(|x| truthy(x)) {
                self.charge(std::mem::size_of::<Form>() * 2 + 12)?;
                let nil = || Form { span: 0..0, metadata: Vec::new(), kind: Kind::Nil };
                let parameters = match &arglists.kind {
                    Kind::List(x) | Kind::Vector(x) | Kind::Set(x) => x.get(1).map(|x| self.clone_form(x)).transpose()?.unwrap_or_else(nil),
                    Kind::Map(x) => if let Some(pair) = x.chunks_exact(2).nth(1) {
                        self.charge(std::mem::size_of::<Vec<Form>>() * 2 + 4)?;
                        let mut forms = Vec::new(); reuse::reserve(self.session, &mut forms, 2)?;
                        let a = self.clone_form(&pair[0])?; let b = self.clone_form(&pair[1])?; self.charge(4)?; forms.push(a); forms.push(b);
                        Form { span: 0..0, metadata: Vec::new(), kind: Kind::Vector(forms) }
                    } else { nil() },
                    Kind::String(x) => if let Some(unit) = x.get(1) { self.charge(12)?; Form { span: 0..0, metadata: Vec::new(), kind: Kind::String(vec![*unit]) } } else { nil() },
                    _ => return Err(SessionError::Host(wasmtime::Error::msg("Declared argument lists require seqable reader data"))),
                };
                let value = self.flag(true)?; self.completed_set(&mut fields, "declared", value)?;
                let value = self.flag(true)?; self.completed_set(&mut fields, "fn-var", value)?;
                let value = self.form(&parameters, depth + 1)?; self.completed_set(&mut fields, "method-params", value)?;
            }
        }
        if !provisional {
            let raw = self.completed_property(&metadata, "tag")?;
            let tag = if owner.function.is_some() { raw.filter(|x| truthy(x)).or(owner.inferred_return.as_ref()) }
                else if raw.is_some_and(truthy) { raw }
                else if self.completed_property(&metadata, "dynamic")?.is_some_and(truthy) { None }
                else { owner.inferred.as_ref().or(raw) };
            let dynamic_any = owner.function.is_none() && !raw.is_some_and(truthy)
                && self.completed_property(&metadata, "dynamic")?.is_some_and(truthy);
            if dynamic_any { let value = self.symbol("any")?; self.completed_set(&mut fields, "tag", value)?; }
            else if let Some(tag) = tag {
                let value = self.form(tag, depth + 1)?;
                self.completed_set(&mut fields, if owner.function.is_some() { "ret-tag" } else { "tag" }, value)?;
            }
        }
        // Complete the original metadata value AFTER any top-fn replacement.
        self.charge(fields.len().saturating_mul(12).saturating_add(4))?;
        if let Some((_, value)) = fields.iter_mut().find(|(name, _)| *name == "meta") {
            *value = self.recipe(Recipe::Alias(*value))?;
        }
        self.charge(fields.len().saturating_mul(std::mem::size_of::<(&str, Value)>() * 2 + 4)
            .saturating_add(std::mem::size_of::<Fields>() * 2 + 4))?;
        let root = self.map(fields.clone())?;
        self.completed_store(global, owner, holder, CompletedCacheKind::Fields, root)?;
        Ok(fields)
    }
    pub(super) fn completed_source_value_recipe(&mut self, global: &portable::resolve::Global,
        holder: &Info, source: &Arc<portable::resolve::TypeSourceValue>, depth: usize) -> Result<Value> {
        self.charge(22 * 8 + global.namespace().len() * 2 + global.name().len() * 2 + 12)?;
        let plan = holder.type_key_history.as_ref().and_then(|h| h.values(global)).ok_or_else(||
            SessionError::Host(wasmtime::Error::msg("Missing current source value plan")))?;
        if !preserve_source_error(holder.declaration.span.clone(), &mut |n| self.session.charge_host_work(n),
            |meter| plan.matches_current_type(holder, meter))? {
            return Err(SessionError::Host(wasmtime::Error::msg("Changed type source value revision")));
        }
        if !plan.overrides().any(|(_, value)| Arc::ptr_eq(value, source)) {
            return Err(SessionError::Host(wasmtime::Error::msg("Foreign type source value owner")));
        }
        self.charge(global.namespace().len().saturating_add(global.name().len()).saturating_mul(2)
            .saturating_add(std::mem::size_of::<DeclarationCacheKey>() * 2 + 16))?;
        let key = DeclarationCacheKey::SourceValue(global.clone(), Arc::as_ptr(source) as usize);
        if let Some((_, value)) = self.declarations.get(&key) { return Ok(*value); }
        validate_form_data(source.form(), &mut |units| self.session.charge_host_work(units))?;
        // Each last-writer value retains ITS original source owner through
        // repeated types; do not enrich an old value using the current origin.
        let form = if let Some(origin) = source.origin() {
            preserve_source_error(source.form().span.clone(), &mut |n| self.session.charge_host_work(n),
                |meter| origin.macro_form_data_metered(source.form(), &mut |work| meter(work)))?
        } else { self.clone_form(source.form())? };
        validate_form_data(&form, &mut |units| self.session.charge_host_work(units))?;
        let value = self.form(&form, depth)?;
        let value = self.recipe(Recipe::Alias(value))?;
        self.charge(global.namespace().len() * 2 + global.name().len() * 2
            + std::mem::size_of::<(DeclarationCacheKey, (Option<Info>, Value))>() * 2 + 24)?;
        self.declarations.insert(key, (Some(holder.clone()), value));
        Ok(value)
    }
    fn completed_assign(&mut self, fields: &mut Fields, name: &'static str, value: Value) -> Result<()> {
        self.charge(4)?;
        for (label, old) in fields.iter_mut() {
            self.charge(label.len().saturating_add(name.len()).saturating_add(4))?;
            if *label == name { *old = value; return Ok(()); }
        }
        reuse::reserve(self.session, fields, 1)?;
        self.charge(4)?; fields.push((name, value)); Ok(())
    }
    pub(super) fn apply_completed_ordinary_values(&mut self, global: &portable::resolve::Global,
        owner: &Arc<CompletedDeclarationSource>, holder: &Info, fields: &mut Fields, depth: usize) -> Result<()> {
        let values = self.completed_field_values(global, owner, holder, depth)?;
        // Existing writer positions/extension fields stay in place. The same
        // completed native value policy drives ordinary and cold type output;
        // the earlier legacy validation walk retains its original error order.
        for label in VALUE_FIELDS {
            self.charge(values.len().saturating_mul(label.len().saturating_add(8)).saturating_add(4))?;
            if let Some(value) = values.iter().find_map(|(name, value)| (*name == label).then_some(*value)) {
                self.completed_assign(fields, label, value)?;
            } else {
                self.charge(fields.len().saturating_mul(label.len().saturating_add(8)).saturating_add(4))?;
                fields.retain(|(name, _)| *name != label);
            }
        }
        Ok(())
    }
    pub(super) fn apply_completed_type_values(&mut self, global: &portable::resolve::Global,
        info: &Info, fields: &mut Fields, depth: usize) -> Result<()> {
        self.charge(8)?;
        let Some(plan) = info.type_key_history.as_ref().and_then(|h| h.values(global)) else { return Ok(()); };
        let mut values = if let Some(owner) = &plan.source { self.completed_field_values(global, owner, info, depth)? }
            else { self.charge(std::mem::size_of::<Fields>() * 2 + 4)?; Vec::new() };
        // parse-type assoc defaults; these replace values, never original keys.
        let name = self.symbol(&format!("{}/{}", global.namespace(), global.name()))?;
        self.completed_assign(&mut values, "name", name)?;
        let tag = self.symbol("function")?; self.completed_assign(&mut values, "tag", tag)?;
        let value = self.flag(true)?; self.completed_assign(&mut values, "type", value)?;
        let count = info.type_fields.ok_or_else(|| SessionError::Host(wasmtime::Error::msg("Type value plan on ordinary declaration")))?;
        let value = self.number(count)?; self.completed_assign(&mut values, "num-fields", value)?;
        let value = self.flag(info.type_is_record)?; self.completed_assign(&mut values, "record", value)?;
        for (label, source) in plan.overrides() {
            self.charge(6)?;
            let value = self.completed_source_value_recipe(global, info, source, depth + 1)?;
            self.completed_assign(&mut values, label, value)?;
        }
        // The original current-source position query has already run, in its
        // old place; retain its actual values/errors rather than querying twice.
        for label in ["line", "column", "file"] {
            self.charge(fields.len().saturating_mul(label.len().saturating_add(8)).saturating_add(4))?;
            if let Some(value) = fields.iter().find_map(|(name, value)| (*name == label).then_some(*value)) {
                self.completed_assign(&mut values, label, value)?;
            }
        }
        for label in VALUE_FIELDS {
            self.charge(values.len().saturating_mul(label.len().saturating_add(8)).saturating_add(4))?;
            let value = values.iter().find_map(|(name, value)| (*name == label).then_some(*value));
            self.charge(fields.len().saturating_mul(label.len().saturating_add(8)).saturating_add(6))?;
            if let Some(value) = value { self.completed_assign(fields, label, value)?; }
            else { fields.retain(|(name, _)| *name != label); }
        }
        Ok(())
    }
}
