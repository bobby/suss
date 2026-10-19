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
    pub(super) fn completed_owner_binding_full(&mut self, global: &portable::resolve::Global,
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
    pub(super) fn completed_reader_source(&mut self, owner: &CompletedDeclarationSource) -> Result<Form> {
        validate_form_data(&owner.definition, &mut |units| self.session.charge_host_work(units))?;
        validate_form_data(&owner.declaration, &mut |units| self.session.charge_host_work(units))?;
        self.charge(6)?;
        let declaration = if let Some(origin) = &owner.origin {
            origin.macro_form_data_metered(&owner.declaration, &mut |units|
                charge_origin_work(self.session, owner.declaration.span.clone(), units)).map_err(SessionError::Compile)?
        } else { self.clone_form(&owner.declaration)? };
        Ok(declaration)
    }
    pub(super) fn completed_reader(&mut self, owner: &CompletedDeclarationSource) -> Result<hir::ReaderMetadata> {
        let declaration = self.completed_reader_source(owner)?;
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
            let key = if self.capture_roots && self.direct_reader_roots {
                self.capture_completed_reader_key(global, owner, holder, metadata.key_positions[index])?
            } else {
                self.reader_key_recipe(&pair[0], metadata.origins[index])?
            };
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
    // One normalized SOURCE occurrence, not one spelling/value. The owner is
    // retained by completed_store and fully bound to the current holder before
    // every lookup. Independent source owners never meet in this registry.
    fn completed_reader_property_value(&mut self, global: &portable::resolve::Global,
        owner: &Arc<CompletedDeclarationSource>, holder: &Info,
        index: usize, depth: usize) -> Result<Value> {
        if let Some(value) = self.completed_lookup(global, owner, holder,
            CompletedCacheKind::ReaderValue(index))? { return Ok(value); }
        let value = if self.capture_roots && self.direct_reader_roots {
            // This constructor accepts only owner/holder/ordinal; it constructs
            // its own native value from the original source. No external ID or
            // Form can manufacture the captured DATA occurrence.
            self.capture_completed_reader_value(global, owner, holder, index, depth)?
        } else {
            let metadata = self.completed_reader(owner)?;
            self.charge(12)?;
            if metadata.pairs.len() % 2 != 0 || metadata.pairs.len() / 2 != metadata.origins.len()
                || metadata.origins.len() != metadata.key_positions.len()
                || index >= metadata.origins.len() {
                return Err(SessionError::Host(wasmtime::Error::msg("Invalid completed reader value ordinal")));
            }
            let native = self.form(&metadata.pairs[index * 2 + 1], depth)?;
            // Metadata/List propagate reader repetition. This barrier gives one
            // immutable source occurrence one construction, even without a cache.
            self.recipe(Recipe::Alias(native))?
        };
        self.completed_store(global, owner, holder, CompletedCacheKind::ReaderValue(index), value)?;
        Ok(value)
    }
    fn completed_property_index(&mut self, metadata: &hir::ReaderMetadata, label: &str) -> Result<Option<usize>> {
        // Entry, selector setup and return controls are paid even when there
        // are zero complete pairs. Do not borrow slot-allocation charges for
        // this separate work. No iterator can advance ahead of its debit.
        self.charge(8)?;
        let mut index = 0;
        loop {
            // Next/probe, extent/index reads, checked-in-range pair offset and
            // indexed slice selection; includes the empty/final no-hit probe.
            self.charge(6)?;
            if index >= metadata.pairs.len() / 2 { return Ok(None); }
            // Preserve the original per-pair comparison charge exactly.
            self.charge(label.len().saturating_add(6))?;
            let pair = &metadata.pairs[index * 2..index * 2 + 2];
            if matches!(&pair[0].kind, Kind::Keyword(k) if k.namespace.is_none() && k.name == label) {
                return Ok(Some(index));
            }
            self.charge(2)?; // increment before the next pair, including no-hit
            index += 1;
        }
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
        let mut values = self.reader_key_slots(entries.len())?;
        // Match precisely the initial 22-field reader writer. Later computed,
        // top-fn, explicit-user-meta, position and inferred writers keep their
        // existing independent provenance and precedence.
        // Prepay all fixed iterator/label/skip/call-branch controls BEFORE the
        // loop starts, including four excluded labels and eighteen empty scans.
        self.charge(8 + VALUE_FIELDS.len() * 8)?;
        for label in VALUE_FIELDS {
            if matches!(label, "meta" | "line" | "column" | "file") { continue; }
            if let Some(index) = self.completed_property_index(&metadata, label)? {
                let value = self.completed_reader_property_value(global, owner, holder, index, depth + 1)?;
                self.charge(2)?;
                values[index] = Some(value);
            }
        }
        let value = self.declaration_reader_metadata_map_with_values(&metadata, &mut keys,
            Some(&values), global.namespace() == "suss.core", depth)?;
        let value = if self.capture_roots && self.direct_reader_roots {
            self.capture_completed_metadata(global, owner, holder, value)?
        } else { value };
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
            if let Some(index) = self.completed_property_index(&metadata, label)? {
                let value = self.completed_reader_property_value(global, owner, holder, index, depth + 1)?;
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
                    let value = if arglists.is_some() {
                        let index = self.completed_property_index(&metadata, "arglists")?.ok_or_else(||
                            SessionError::Host(wasmtime::Error::msg("Missing selected source arglists")))?;
                        self.completed_reader_property_value(global, owner, holder, index, depth + 1)?
                    } else { self.scalar(Literal::Nil)? };
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
            // Carry the actual writer decision, never Form equality or native
            // pointer coincidence. Inferred/dynamic values are separate writers.
            let (tag, reader_writer) = if owner.function.is_some() {
                if raw.is_some_and(truthy) { (raw, true) } else { (owner.inferred_return.as_ref(), false) }
            } else if raw.is_some_and(truthy) { (raw, true) }
            else if self.completed_property(&metadata, "dynamic")?.is_some_and(truthy) { (None, false) }
            else if let Some(inferred) = owner.inferred.as_ref() { (Some(inferred), false) }
            else { (raw, raw.is_some()) };
            let dynamic_any = owner.function.is_none() && !raw.is_some_and(truthy)
                && self.completed_property(&metadata, "dynamic")?.is_some_and(truthy);
            if dynamic_any { let value = self.symbol("any")?; self.completed_set(&mut fields, "tag", value)?; }
            else if let Some(tag) = tag {
                let value = if reader_writer {
                    let index = self.completed_property_index(&metadata, "tag")?.ok_or_else(||
                        SessionError::Host(wasmtime::Error::msg("Missing selected source tag")))?;
                    self.completed_reader_property_value(global, owner, holder, index, depth + 1)?
                } else { self.form(tag, depth + 1)? };
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
    pub(super) fn check_explicit_source_binding(&mut self, global: &portable::resolve::Global,
        holder: &Info, source: &Arc<portable::resolve::TypeSourceValue>) -> Result<()> {
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
        Ok(())
    }
    pub(super) fn completed_source_value_recipe(&mut self, global: &portable::resolve::Global,
        holder: &Info, source: &Arc<portable::resolve::TypeSourceValue>, depth: usize) -> Result<Value> {
        self.check_explicit_source_binding(global, holder, source)?;
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
        let value = if self.capture_roots && self.direct_reader_roots {
            self.capture_explicit_source_value(global, source, holder, value)?
        } else { value };
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

#[cfg(test)]
mod reader_property_value_tests {
    use super::*;
    use portable::resolve::{Environment, Global, Phase};

    fn prepare(source: &str, phase: Phase, environment: &Environment, indexed: bool)
        -> (Environment, Global, Info) {
        let forms = suss_reader::forms::read_forms(source).unwrap();
        let origin = indexed.then(|| portable::SourceOrigin::new(source, Some("value-owner.cljs".into())));
        let environment = hir::prepare_with_origin(&forms, 0..source.len(), environment, phase,
            &mut portable::NoExpansion, origin.as_ref()).unwrap().1;
        let binding = environment.resolve(phase, &suss_reader::Symbol::new("subject"), 0..0).unwrap();
        let global = binding.global().clone();
        let info = environment.shared_definition_info(&global).unwrap().clone();
        (environment, global, info)
    }
    fn selected(_graph: &mut AnalysisGraph<'_>, fields: &Fields, label: &str) -> Value {
        fields.iter().find_map(|(name, value)| (*name == label).then_some(*value))
            .unwrap_or_else(|| panic!("required field {label}"))
    }
    fn property(graph: &mut AnalysisGraph<'_>, metadata: Value, label: &str) -> Value {
        let entries = graph.completed_pairs(metadata).unwrap();
        graph.declaration_field_value(&entries, label).unwrap().expect("selected metadata value")
    }

    #[test]
    fn completed_reader_property_occurrences_share_outer_inner_objects_without_symbol_only_rule() {
        // The raw doc and arglists programs are existing ordinary-writer control
        // shapes, not newly assumed guest prototype semantics. Outer/inner are
        // independently obtained from the actual field and metadata producers.
        let cases = [
            ("(def ^{:doc raw-doc :meta {:x 1} :fn-var false :variadic? false} subject 17)", "doc"),
            ("(def ^{:arglists '([^{:note 7} x])} subject (fn* [x] x))", "arglists"),
            ("(def ^{:export [17 23]} subject 17)", "export"),
            ("(def ^number subject 17)", "tag"),
            ("(def ^{:ret-tag ^{:marker :kept} result-type} subject 17)", "ret-tag"),
        ];
        for (phase, mut session) in [(Phase::Runtime, Session::new_repl().unwrap()),
            (Phase::Macro, Session::new_macro().unwrap())] {
            let bridge = FormBridge::new(&mut session).unwrap();
            let same = session.eval("(fn [v] (identical? (nth v 0) (nth v 1)))").unwrap();
            for (source, label) in cases {
                for indexed in [false, true] {
                    let (_environment, global, info) = prepare(source, phase, &Environment::default(), indexed);
                    let owner = info.completed_values.as_ref().unwrap();
                    let original = info.declaration.clone();
                    let values = {
                        let mut graph = AnalysisGraph::new(&bridge, &mut session);
                        let fields = graph.completed_field_values(&global, owner, &info, 0).unwrap();
                        let outer = selected(&mut graph, &fields, label);
                        let meta = selected(&mut graph, &fields, "meta");
                        let inner = property(&mut graph, meta, label);
                        // Native occurrence identity is supplemental evidence;
                        // the actual compiled identity check below is mandatory.
                        assert_eq!(outer, inner, "same original writer: {label}");
                        let root = graph.vector(&[outer, inner]).unwrap();
                        graph.materialize(root).unwrap()
                    };
                    session.collect().unwrap();
                    let actual = session.invoke(&same, &[&values]).unwrap();
                    assert_eq!(bridge.read(&mut session, &actual, 0..0).unwrap().kind,
                        Kind::Bool(true), "{phase:?} indexed={indexed} {label}: {source}");
                    assert_eq!(info.declaration, original);
                }
            }
        }
    }

    fn retained_values(session: &mut Session, bridge: &FormBridge, cache: &mut source_roots::RootCache,
        global: &Global, prior: &Info, current: &Info, current_first: bool, metadata_first: bool)
        -> crate::portable_session::SessionValue {
        let owner = prior.completed_values.as_ref().unwrap();
        let mut graph = AnalysisGraph::new(bridge, session);
        graph.capture_roots = true;
        graph.direct_reader_roots = true;
        // The genuine current type's collapsed source still owns the original
        // ordinary declaration. Exercise both producer orders in one Graph.
        let first = if current_first { current } else { prior };
        let second = if current_first { prior } else { current };
        let a = graph.completed_field_values(global, owner, first, 0).unwrap();
        let b = graph.completed_field_values(global, owner, second, 0).unwrap();
        let meta = selected(&mut graph, &a, "meta");
        let inner = property(&mut graph, meta, "tag");
        let tag = selected(&mut graph, &a, "tag");
        let ret = selected(&mut graph, &b, "ret-tag");
        assert_eq!(tag, inner);
        assert_eq!(ret, inner, "raw source tag is also the actual function return writer");
        // This is the important warm case: selecting the retained Metadata root
        // before its value cuts traversal. Its captured VALUE child must bind
        // the later outer export; an uncaptured native ID would reconstruct it.
        let values = if metadata_first { [meta, tag, ret] } else { [tag, meta, ret] };
        let root = graph.vector(&values).unwrap();
        assert!(graph.admit_reader_controller(root).unwrap().is_none(),
            "a DATA value slot cannot mint a strict source certificate");
        graph.materialize_reader_controller(root, cache).unwrap()
            .expect("healthy original-source DATA identity must not decline")
    }

    #[test]
    fn completed_property_data_slots_preserve_cold_warm_current_prior_and_retained_parent_cuts_after_gc() {
        for (phase, mut session) in [(Phase::Runtime, Session::new_repl().unwrap()),
            (Phase::Macro, Session::new_macro().unwrap())] {
            let bridge = FormBridge::new(&mut session).unwrap();
            for indexed in [false, true] {
                let (prior_environment, global, prior) = prepare("(def ^number subject (fn* [] 17))",
                    phase, &Environment::default(), indexed);
                let (_current_environment, same_global, current) = prepare("(deftype* subject [] nil nil)",
                    phase, &prior_environment, indexed);
                assert_eq!(global, same_global);
                let owner = prior.completed_values.as_ref().unwrap();
                assert!(Arc::ptr_eq(current.type_key_history.as_ref().unwrap().values(&global)
                    .unwrap().source.as_ref().unwrap(), owner));
                for current_first in [false, true] {
                    let mut cache = source_roots::RootCache::default();
                    let mut previous = None;
                    for metadata_first in [true, true, false, true] {
                        let result = retained_values(&mut session, &bridge, &mut cache, &global,
                            &prior, &current, current_first, metadata_first);
                        session.collect().unwrap();
                        let get = session.eval(if metadata_first {
                            "(fn [v] [(get (nth v 0) :tag) (nth v 1) (nth v 2)])"
                        } else { "(fn [v] [(get (nth v 1) :tag) (nth v 0) (nth v 2)])" }).unwrap();
                        let row = session.invoke(&get, &[&result]).unwrap();
                        let check = session.eval("(fn [v] (and (identical? (nth v 0) (nth v 1)) (identical? (nth v 1) (nth v 2))))").unwrap();
                        let passed = session.invoke(&check, &[&row]).unwrap();
                        assert_eq!(bridge.read(&mut session, &passed, 0..0).unwrap().kind, Kind::Bool(true),
                            "{phase:?} indexed={indexed} current_first={current_first} metadata_first={metadata_first}");
                        let first = session.eval("(fn [v] (nth v 0))").unwrap();
                        let current_value = session.invoke(&first, &[&row]).unwrap();
                        if let Some(old) = &previous {
                            assert!(session.values_identical(old, &current_value).unwrap(),
                                "same source value must retain identity across healthy warm requests");
                        }
                        previous = Some(current_value);
                    }
                }
            }
        }
    }

    #[test]
    fn completed_reader_values_reject_foreign_equal_owner_changed_source_and_unknown_holder_before_publication() {
        for (phase, mut session) in [(Phase::Runtime, Session::new_repl().unwrap()),
            (Phase::Macro, Session::new_macro().unwrap())] {
            let bridge = FormBridge::new(&mut session).unwrap();
            let source = "(def ^number subject 17)";
            let (_a, global, info) = prepare(source, phase, &Environment::default(), true);
            let (_b, _, foreign) = prepare(source, phase, &Environment::default(), true);
            let owner = info.completed_values.as_ref().unwrap();
            let foreign_owner = foreign.completed_values.as_ref().unwrap();
            assert!(!Arc::ptr_eq(owner, foreign_owner));
            let mut changed = info.as_ref().clone();
            changed.declaration.metadata.push(suss_reader::forms::read_forms("{:tag changed}").unwrap().remove(0));
            let changed = Arc::new(changed);
            let mut unknown = info.as_ref().clone();
            unknown.completed_values = None;
            let unknown = Arc::new(unknown);
            let mut graph = AnalysisGraph::new(&bridge, &mut session);
            graph.capture_roots = true;
            graph.direct_reader_roots = true;
            let metadata = graph.completed_reader(owner).unwrap();
            let index = graph.completed_property_index(&metadata, "tag").unwrap().unwrap();
            for (candidate, holder) in [(foreign_owner, &info), (owner, &changed), (owner, &unknown)] {
                let before = graph.recipes.len();
                assert!(matches!(graph.completed_reader_property_value(&global, candidate, holder, index, 0),
                    Err(SessionError::Host(_))));
                assert_eq!(graph.recipes.len(), before, "invalid ownership cannot publish a value recipe");
            }
            // A cache hit is not authority either: mutate after the successful
            // source lookup, then recheck the full holder binding on that hit.
            graph.completed_reader_property_value(&global, owner, &info, index, 0).unwrap();
            let before = graph.recipes.len();
            assert!(matches!(graph.completed_reader_property_value(&global, owner, &changed, index, 0),
                Err(SessionError::Host(_))));
            assert_eq!(graph.recipes.len(), before);
        }
    }

    #[test]
    fn reader_value_coordinate_tracks_winning_writer_independently_from_retained_key() {
        let form = suss_reader::forms::read_forms("^number ^{:tag other :doc raw-doc} subject").unwrap().remove(0);
        let entries = hir::reader_metadata_view_metered(&form, &mut |_| Ok(())).unwrap();
        let tag = entries.iter().find(|entry| matches!(&entry.key.as_ref().kind,
            Kind::Keyword(k) if k.namespace.is_none() && k.name == "tag")).unwrap();
        assert_ne!(tag.key_position(), tag.value_position(), "outer association replaces only the value");
        assert!(matches!(&tag.value.as_ref().kind, Kind::Symbol(s) if s.name == "number"));
        let (prefix, pair) = tag.value_position();
        assert_eq!(pair, 0);
        assert!(matches!(&form.metadata[prefix].kind, Kind::Symbol(s) if s.name == "number"));
    }

    #[test]
    fn reader_property_empty_and_no_hit_scans_bill_work_and_preserve_typed_failure_irq_and_retry() {
        let mut session = Session::new_macro().unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        for source in ["(def subject 17)", "(def ^{:private true} subject 17)"] {
            let (_environment, global, info) = prepare(source, Phase::Macro, &Environment::default(), false);
            let original = info.declaration.clone();
            let metadata = {
                let mut graph = AnalysisGraph::new(&bridge, &mut session);
                graph.completed_reader(info.completed_values.as_ref().unwrap()).unwrap()
            };
            let request = session.operation_budget_scope().unwrap();
            let used = {
                let mut graph = AnalysisGraph::new(&bridge, &mut session);
                let before = graph.session.operation_fuel_remaining().unwrap();
                assert_eq!(graph.completed_property_index(&metadata, "doc").unwrap(), None);
                before - graph.session.operation_fuel_remaining().unwrap()
            };
            drop(request);
            assert!(used > 0, "a genuine empty/no-hit source scan performs paid work");
            // No exact fee formula is repeated here. The measured actual work
            // must fail atomically under insufficient budget and retry fully.
            for allowance in [0, 1, used - 1] {
                session.set_operation_fuel(allowance);
                let request = session.operation_budget_scope().unwrap();
                let error = {
                    let mut graph = AnalysisGraph::new(&bridge, &mut session);
                    graph.completed_property_index(&metadata, "doc").unwrap_err()
                };
                assert!(matches!(error, SessionError::Trap(ref e)
                    if e.downcast_ref::<wasmtime::Trap>() == Some(&wasmtime::Trap::OutOfFuel)), "{error:?}");
                drop(request);
                assert_eq!(info.declaration, original);
                session.set_operation_fuel(crate::portable::DEFAULT_OPERATION_FUEL);
                let request = session.operation_budget_scope().unwrap();
                let mut graph = AnalysisGraph::new(&bridge, &mut session);
                assert_eq!(graph.completed_property_index(&metadata, "doc").unwrap(), None);
                drop(graph);
                drop(request);
            }
            let request = session.operation_budget_scope().unwrap();
            session.interrupt_handle().interrupt(); // active scope, not cleared by begin
            let error = {
                let mut graph = AnalysisGraph::new(&bridge, &mut session);
                graph.completed_property_index(&metadata, "doc").unwrap_err()
            };
            assert!(error.is_interrupt(), "{error:?}");
            drop(request);
            let request = session.operation_budget_scope().unwrap();
            let mut graph = AnalysisGraph::new(&bridge, &mut session);
            assert_eq!(graph.completed_property_index(&metadata, "doc").unwrap(), None);
            // A real full producer remains valid after the failed lookups; no
            // synthetic source owner or preselected field value is injected.
            let meta = graph.completed_metadata_map(&global, info.completed_values.as_ref().unwrap(), &info, 0).unwrap();
            assert!(graph.completed_pairs(meta).is_ok());
            drop(graph);
            drop(request);
            assert_eq!(info.declaration, original);
        }
    }
}
