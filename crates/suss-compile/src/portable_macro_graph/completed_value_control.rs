//! Test-only independent original ordinary writer: exact base74 method except
//! name and disjoint enum cache key. No copied codec or runtime meter changes.
use super::*;
impl AnalysisGraph<'_> {
    fn legacy_declaration(
        &mut self,
        global: &portable::resolve::Global,
        info: Option<&Arc<portable::resolve::DefinitionInfo>>,
        depth: usize,
    ) -> Result<Value> {
        Self::depth(depth)?;
        // Own each revision so addresses cannot be recycled within this graph.
        // Global identity also distinguishes plain catalog entries without facts.
        let key = DeclarationCacheKey::LegacyRevision(
            global.clone(), info.map_or(0, |info| Arc::as_ptr(info) as usize));
        self.charge(global.namespace().len().saturating_add(global.name().len()).saturating_add(1))?;
        if let Some((_, value)) = self.declarations.get(&key) {
            return Ok(*value);
        }
        let tracing = std::env::var_os("SUSS_METADATA_FUEL_TRACE").is_some();
        let trace = |session: &Session, stage: &str| -> Result<()> {
            if tracing {
                eprintln!("metadata-declaration {}/{} {stage}: {}", global.namespace(), global.name(), session.operation_fuel_remaining()?);
            }
            Ok(())
        };
        trace(self.session, "start")?;
        let name = self.symbol(&format!("{}/{}", global.namespace(), global.name()))?;
        self.charge(24)?; // native definition/shape/role reads and sidecar branch
        let prefixes = info.map_or(0, |info| info.declaration.metadata.len());
        self.charge(prefixes.saturating_mul(3))?; // all prefix-kind reads/checks before scanning
        let ordinary_def = info.is_some_and(|info| info.type_fields.is_none()
            // Reader shorthand eligibility uses the actual normalized entry
            // origin below, never a synthesized Form or tag spelling alone.
            && info.declaration.metadata.iter().all(|prefix|
                matches!(&prefix.kind, Kind::Map(_) | Kind::Keyword(_) | Kind::Symbol(_) | Kind::String(_)))
            && matches!(&info.definition_form.kind, Kind::List(items)
                if matches!(items.first().map(|head| &head.kind), Some(Kind::Symbol(head))
                    if head.namespace.is_none() && matches!(head.name.as_str(), "def" | "defonce"))
                && matches!(items.get(1).map(|name| &name.kind), Some(Kind::Symbol(_)))));
        let mut retained = if ordinary_def {
            let mut keys = RetainedFieldKeys::new(self)?;
            keys.remember_named(self, AnalyzerLiteralKeyword("name"))?;
            Some(keys)
        } else if info.is_some_and(|info| info.type_fields.is_some()) {
            Some(RetainedFieldKeys::type_history(self)?)
        } else { None };
        let mut fields = vec![("name", name)];
        if let Some(info) = info {
            // Preserve the transport guard before requesting reader enrichment.
            // An oversized metadata value must not reach source snapshots or
            // guest construction merely because only selected fields are read.
            validate_form_data(&info.definition_form, &mut |units| {
                self.session.charge_host_work(units)
            })?;
            validate_form_data(&info.declaration, &mut |units| {
                self.session.charge_host_work(units)
            })?;
            trace(self.session, "guarded")?;
            self.charge(3)?;
            let (mut declaration, mut owned_reader_prefixes) = if let Some(origin) = &info.origin {
                origin
                    .macro_form_data_with_metadata_origins_metered(&info.declaration, &mut |units| {
                        charge_origin_work(self.session, info.declaration.span.clone(), units)
                    })
                    .map_err(SessionError::Compile)?
            } else {
                (self.clone_form(&info.declaration)?, Some(info.declaration.metadata.len()))
            };
            trace(self.session, "declaration-origin")?;
            // declare adds semantic metadata to an original source symbol. Only
            // recover positions through the verified original declaration form,
            // never by trusting a generated symbol's reused byte span alone.
            if let (Some(origin), Kind::List(items)) = (&info.origin, &info.definition_form.kind) {
                if items.first().is_some_and(|head| {
                    matches!(&head.kind, Kind::Symbol(symbol)
                    if symbol.namespace.is_none() && symbol.name == "declare")
                }) {
                    if let Some(original) = items[1..].iter().find(|name| {
                        name.kind == info.declaration.kind && name.span == info.declaration.span
                    }) {
                        let mut original = origin
                            .macro_form_data_metered(original, &mut |units| {
                                charge_origin_work(self.session, original.span.clone(), units)
                            })
                            .map_err(SessionError::Compile)?;
                        self.charge(1)?;
                        owned_reader_prefixes = None;
                        declaration.metadata = std::mem::take(&mut original.metadata);
                        for metadata in &info.declaration.metadata {
                            declaration.metadata.push(self.clone_form(metadata)?);
                        }
                    }
                }
            }
            let reader_metadata =
                crate::portable_macro_data::normalized_reader_metadata_view(self.session, &declaration)?;
            let metadata = &reader_metadata.pairs;
            let mut metadata_keys = self.reader_key_slots(reader_metadata.origins.len())?;
            let property = |name: &str| {
                metadata.chunks_exact(2).find_map(|pair| {
                    matches!(&pair[0].kind, Kind::Keyword(key)
                    if key.namespace.is_none() && key.name == name)
                    .then_some(&pair[1])
                })
            };
            // Source KEY facts come from the original owned declaration, not
            // synthetic line/file entries appended by source-origin enrichment.
            // The original full guards/enrichment/normalization have run first.
            self.charge(2)?;
            if ordinary_def && let Some(keys) = retained.as_mut() {
                // Consume ONE owned normalized view. The source clone recorded
                // which prefix slots belong to the actual selected reader
                // clone input. Normalization retains the KEY's (prefix,pair)
                // selector even when a later prefix replaced its VALUE.
                self.charge(3)?;
                if let Some(original_count) = owned_reader_prefixes {
                    for label in ["name", "ns", "private", "dynamic", "doc", "declared", "tag", "ret-tag",
                        "fn-var", "variadic?", "max-fixed-arity", "method-params", "arglists", "arglists-meta",
                        "export", "type", "num-fields", "record", "meta"] {
                        self.charge(2)?;
                        for (index, pair) in metadata.chunks_exact(2).enumerate() {
                            self.charge(label.len().saturating_add(8))?;
                            if reader_metadata.key_positions[index].0 < original_count
                                && matches!(&pair[0].kind, Kind::Keyword(key)
                                    if key.namespace.is_none() && key.name == label) {
                                let recipe = self.reader_key_recipe(&pair[0], reader_metadata.origins[index])?;
                                metadata_keys[index] = Some(recipe);
                                keys.remember_reader_occurrence(self, label, &pair[0], recipe)?;
                                break;
                            }
                        }
                    }
                    // Root position enrichment is not assessed by the tag
                    // reference. Preserve its ORIGINAL raw-key registration
                    // policy without joining it to an enriched occurrence.
                    let position_keys = preserve_source_error(info.declaration.span.clone(),
                        &mut |units| self.session.charge_host_work(units),
                        |meter| hir::reader_metadata_view_metered(&info.declaration, meter))?;
                    for label in ["line", "column", "file"] {
                        for entry in &position_keys {
                            self.charge(label.len().saturating_add(6))?;
                            if matches!(&entry.key.as_ref().kind, Kind::Keyword(key)
                                if key.namespace.is_none() && key.name == label) {
                                keys.remember_source(self, label, entry.key.as_ref())?; break;
                            }
                        }
                    }
                } else {
                    // Only non-Symbol/unsupported native root shapes can lack
                    // a Symbol prefix-owner proof. All selected Symbol readers,
                    // including eager/paired mappings, take the owned path.
                    // Do not join equal keys from separate views here.
                    let raw = preserve_source_error(info.declaration.span.clone(),
                        &mut |units| self.session.charge_host_work(units),
                        |meter| hir::reader_metadata_pairs_metered(&info.declaration, meter))?;
                    for label in ["name", "ns", "private", "dynamic", "doc", "declared", "tag", "ret-tag",
                        "fn-var", "variadic?", "max-fixed-arity", "method-params", "arglists", "arglists-meta",
                        "export", "type", "num-fields", "record", "meta", "line", "column", "file"] {
                        for pair in raw.chunks_exact(2) {
                            self.charge(label.len().saturating_add(4))?;
                            if matches!(&pair[0].kind, Kind::Keyword(key) if key.namespace.is_none() && key.name == label) {
                                keys.remember_source(self, label, &pair[0])?; break;
                            }
                        }
                    }
                }
            }
            self.charge(2)?;
            if info.type_fields.is_some() && let Some(keys) = retained.as_mut() {
                self.seed_type_key_history(keys, global, info)?;
            }
            let truthy = |form: &Form| !matches!(form.kind, Kind::Nil | Kind::Bool(false));
            if let Some(count) = info.type_fields {
                fields.extend([
                    ("type", self.flag(true)?),
                    ("num-fields", self.number(count)?),
                    ("record", self.flag(info.type_is_record)?),
                ]);
            }
            // Namespace declaration records do not have the :ns field of a
            // resolved var AST. Preserve observed raw metadata in either stage.
            for field in [
                "name",
                "ns",
                "private",
                "dynamic",
                "doc",
                "declared",
                "tag",
                "ret-tag",
                "fn-var",
                "variadic?",
                "max-fixed-arity",
                "method-params",
                "arglists",
                "arglists-meta",
                "export",
                "type",
                "num-fields",
                "record",
            ] {
                if let Some(value) = property(field) {
                    fields.retain(|(name, _)| *name != field);
                    fields.push((field, self.form(value, depth + 1)?));
                }
            }
            let provisional = !info.analysis_completed
                || (info.initializer_form.is_some() && property("declared").is_some_and(truthy));
            if !provisional {
                let meta = self.declaration_reader_metadata_map(
                    &reader_metadata, &mut metadata_keys, global.namespace() == "suss.core", depth + 1)?;
                retained_decl_push!(self, retained, fields; "meta", AnalyzerLiteralKeyword("meta"), meta);
            } else if let Some(value) = property("meta") {
                fields.push(("meta", self.form(value, depth + 1)?));
            }
            trace(self.session, "metadata")?;
            if let Some(origin) = &info.origin {
                let (span, metadata) = origin
                    .macro_form_root_metadata_metered(&info.definition_form, &mut |units| {
                        charge_origin_work(self.session, info.definition_form.span.clone(), units)
                    })
                    .map_err(SessionError::Compile)?;
                // Normalization reads metadata only. This local carrier is not
                // quoted or exposed as a replacement definition form.
                let definition = Form { span, metadata, kind: Kind::Nil };
                let position =
                    crate::portable_macro_data::normalized_reader_metadata(self.session, &definition)?;
                for field in ["line", "column", "file"] {
                    if let Some(value) = position.chunks_exact(2).find_map(|pair| {
                        matches!(&pair[0].kind, Kind::Keyword(key)
                            if key.namespace.is_none() && key.name == field)
                        .then_some(&pair[1])
                    }) {
                    let value = self.form(value, depth + 1)?;
                        self.charge(2)?;
                        if let Some(keys) = retained.as_mut() {
                            let token = match field {
                                "line" => AnalyzerLiteralKeyword("line"),
                                "column" => AnalyzerLiteralKeyword("column"),
                                "file" => AnalyzerLiteralKeyword("file"),
                                _ => unreachable!("fixed source-info positions"),
                            };
                            keys.remember_named(self, token)?;
                        }
                        fields.push((field, value));
                    }
                }
            }
            trace(self.session, "definition-origin")?;
            if global.namespace() == "suss.core" {
                fields.retain(|(name, _)| *name != "file");
                retained_decl_push!(self, retained, fields; "file", AnalyzerLiteralKeyword("file"),
                    self.scalar(Literal::String("cljs/core.cljs".encode_utf16().collect()))?);
            }
            let source = info
                .initializer
                .as_ref()
                .and_then(|init| init.source.as_ref());
            let callable = source.and_then(|source| source.callable.as_ref());
            // parse-def chooses a callable return independently of dynamic
            // var-reference inference. False hints fall through just like nil.
            let completed_tag = if info.type_fields.is_some() {
                hir::declaration_tag(info).map_err(SessionError::Compile)?
            } else if provisional {
                None
            } else if callable.is_some() {
                property("tag")
                    .filter(|tag| truthy(tag))
                    .cloned()
                    .or_else(|| {
                        source
                            .and_then(|source| source.tags.inferred_return.as_ref())
                            .and_then(|tag| tag.clone())
                    })
            } else if property("tag").is_some_and(|tag| matches!(tag.kind, Kind::Bool(false))) {
                if property("dynamic").is_some_and(truthy) {
                    Some(Form {
                        span: 0..0,
                        metadata: vec![],
                        kind: Kind::Symbol(suss_reader::Symbol::new("any")),
                    })
                } else {
                    source.and_then(|source| source.tags.inferred_ref().cloned())
                }
            } else {
                hir::declaration_tag(info)
                    .map_err(|error| SessionError::Host(wasmtime::Error::msg(error.message)))?
            };
            let (definition, initializer_recipe) = self.definition_with_initializer(
                &info.definition_form, info.initializer_form.as_ref(), depth + 1,
            )?;
            if tracing && info.initializer_form.is_some() {
                eprintln!("metadata-initializer {}/{} reused={}", global.namespace(), global.name(), initializer_recipe.is_some());
            }
            fields.push(("suss/definition-form", definition));
            fields.push((
                "suss/analysis-completed",
                self.flag(info.analysis_completed)?,
            ));
            fields.push(("suss/declaration", self.form(&info.declaration, depth + 1)?));
            fields.push(("suss/defonce", self.flag(info.once)?));
            if let Some(form) = &info.initializer_form {
                let value = match initializer_recipe {
                    Some(value) => value,
                    None => self.form(form, depth + 1)?,
                };
                fields.push(("suss/initializer-form", value));
            }
            fields.push((
                "suss/initializer-recorded",
                self.flag(info.initializer.is_some())?,
            ));
            if !provisional && let Some(doc) = &info.docstring {
                fields.retain(|(name, _)| *name != "doc");
                retained_decl_push!(self, retained, fields; "doc", AnalyzerLiteralKeyword("doc"), self.scalar(Literal::String(doc.clone()))?);
            }
            if let Some(callable) = callable {
                fields.push(("suss/source-function", self.callable(callable)?));
                const FUNCTION_FIELDS: [&str; 6] = [
                    "fn-var",
                    "variadic?",
                    "max-fixed-arity",
                    "method-params",
                    "arglists",
                    "arglists-meta",
                ];
                if property("declared").is_some_and(truthy) {
                    // The pinned analyzer leaves this initializer's provisional
                    // symbol metadata in the catalog. Completed source callable
                    // facts above remain a separate Suss extension.
                    // The raw selected fields were already copied above, even
                    // without a source callable in a provisional initializer.
                } else if let Some(top) =
                    property("top-fn").filter(|form| !matches!(form.kind, Kind::Nil))
                {
                    // if-some distinguishes nil from an empty map. The map
                    // replaces the computed arity/arglist group and can also
                    // override fn-var; retain selected field presence and data.
                    let pairs: Vec<(&Form, &Form)> = match &top.kind {
                        Kind::Map(items) => items
                            .chunks_exact(2)
                            .map(|pair| (&pair[0], &pair[1]))
                            .collect(),
                        // Pinned merge conjoins a direct vector as one entry.
                        Kind::Vector(items) if items.len() == 2 => {
                            vec![(&items[0], &items[1])]
                        }
                        // Empty seqable values conjoin no entries. Nonempty
                        // reader sequences cannot contain JVM Map$Entry values.
                        Kind::List(items) | Kind::Set(items) if items.is_empty() => vec![],
                        Kind::String(units) if units.is_empty() => vec![],
                        _ => {
                            return Err(SessionError::Host(wasmtime::Error::msg(
                                "Function declaration top-fn requires mergeable reader data",
                            )));
                        }
                    };
                    let macro_flag = property("macro").is_some_and(truthy);
                    fields.retain(|(name, _)| *name != "fn-var");
                    retained_decl_push!(self, retained, fields; "fn-var", AnalyzerLiteralKeyword("fn-var"), self.flag(!macro_flag)?);
                    for field in [
                        "name",
                        "ns",
                        "tag",
                        "ret-tag",
                        "fn-var",
                        "variadic?",
                        "max-fixed-arity",
                        "method-params",
                        "arglists",
                        "arglists-meta",
                        "private",
                        "dynamic",
                        "doc",
                        "declared",
                        "line",
                        "column",
                        "file",
                        "meta",
                    ] {
                        if let Some(form) = pairs.iter().rev().find_map(|(key, value)| {
                            matches!(&key.kind, Kind::Keyword(key)
                                if key.namespace.is_none() && key.name == field)
                            .then_some(*value)
                        }) {
                            fields.retain(|(name, _)| *name != field);
                            let value = self.form(form, depth + 1)?;
                            self.charge(2)?;
                            if let Some(keys) = retained.as_mut() {
                                // `pairs` is verified native top-fn reader data.
                                // First equal KEY survives; the value above is
                                // still selected by the original reverse scan.
                                let mut source_key = None;
                                for (key, _) in &pairs {
                                    self.charge(field.len().saturating_add(4))?;
                                    if matches!(&key.kind, Kind::Keyword(key)
                                        if key.namespace.is_none() && key.name == field) {
                                        source_key = Some(*key); break;
                                    }
                                }
                                if let Some(key) = source_key { keys.remember_source(self, field, key)?; }
                            }
                            fields.push((field, value));
                        }
                    }
                } else {
                    // Normal callable defaults replace the selected raw group.
                    // Empty top-fn instead leaves its five raw arity fields.
                    fields.retain(|(name, _)| !FUNCTION_FIELDS.contains(name));
                    let macro_flag = property("macro")
                        .is_some_and(|value| !matches!(value.kind, Kind::Nil | Kind::Bool(false)));
                    retained_decl_push!(self, retained, fields; "fn-var", AnalyzerLiteralKeyword("fn-var"), self.flag(!macro_flag)?);
                    retained_decl_push!(self, retained, fields; "variadic?", AnalyzerLiteralKeyword("variadic?"), self.flag(callable.variadic())?);
                    let fixed = callable.max_fixed_arity().ok_or_else(|| {
                        SessionError::Host(wasmtime::Error::msg(
                            "Invalid source function declaration",
                        ))
                    })?;
                    retained_decl_push!(self, retained, fields; "max-fixed-arity", AnalyzerLiteralKeyword("max-fixed-arity"), self.number(fixed)?);
                    // Preserve every analyzed source signature, including duplicate
                    // arities. Emitted dispatch methods are a separate contract.
                    let mut methods = Vec::new();
                    for method in &callable.methods {
                        let parameters = method
                            .parameters
                            .iter()
                            .map(|parameter| self.symbol(&parameter.name))
                            .collect::<Result<Vec<_>>>()?;
                        methods.push(self.vector(&parameters)?);
                    }
                    self.charge(0)?;
                    retained_decl_push!(self, retained, fields; "method-params", AnalyzerLiteralKeyword("method-params"), self.recipe(Recipe::List(methods))?);
                    // The pinned analyzer retains the reader value of :arglists,
                    // including its quote, and maps meta over that value's elements.
                    let arglists = property("arglists");
                    let argument_data = if let Some(form) = arglists {
                        self.form(form, depth + 1)?
                    } else {
                        self.scalar(Literal::Nil)?
                    };
                    retained_decl_push!(self, retained, fields; "arglists", AnalyzerLiteralKeyword("arglists"), argument_data);
                    let mut argument_metadata = Vec::new();
                    if let Some(form) = arglists {
                        let items = match &form.kind {
                            Kind::List(items) | Kind::Vector(items) | Kind::Set(items) => {
                                items.as_slice()
                            }
                            // Map sequence entries and UTF-16 character values have
                            // no metadata, unlike original reader collection items.
                            Kind::Map(items) => {
                                for _ in 0..items.len() / 2 {
                                    argument_metadata.push(self.scalar(Literal::Nil)?);
                                }
                                &[]
                            }
                            Kind::String(units) => {
                                for _ in units {
                                    argument_metadata.push(self.scalar(Literal::Nil)?);
                                }
                                &[]
                            }
                            Kind::Nil => &[],
                            _ => {
                                return Err(SessionError::Host(wasmtime::Error::msg(
                                    "Function declaration arglists requires seqable reader data",
                                )));
                            }
                        };
                        for item in items {
                            let pairs =
                                crate::portable_macro_data::normalized_reader_metadata(self.session, item)?;
                            let value = if pairs.is_empty() {
                                self.scalar(Literal::Nil)?
                            } else {
                                self.form(
                                    &Form {
                                        span: item.span.clone(),
                                        metadata: vec![],
                                        kind: Kind::Map(pairs),
                                    },
                                    depth + 1,
                                )?
                            };
                            argument_metadata.push(value);
                        }
                    }
                    self.charge(0)?;
                    retained_decl_push!(self, retained, fields; "arglists-meta", AnalyzerLiteralKeyword("arglists-meta"),
                        self.recipe(Recipe::List(argument_metadata))?);
                }
            }
            if !provisional
                && property("declared").is_some_and(truthy)
                && let Some(arglists) = property("arglists").filter(|value| truthy(value))
            {
                // Fresh forward declaration metadata supplies method data, not
                // an analyzed initializer or an executable source callable.
                // Match parse-def's second over the retained reader value.
                let nil = || Form {
                    span: 0..0,
                    metadata: vec![],
                    kind: Kind::Nil,
                };
                let parameters = match &arglists.kind {
                    Kind::List(items) | Kind::Vector(items) | Kind::Set(items) => {
                        items.get(1).cloned().unwrap_or_else(nil)
                    }
                    Kind::Map(items) => {
                        items.chunks_exact(2).nth(1).map_or_else(nil, |pair| Form {
                            span: 0..0,
                            metadata: vec![],
                            kind: Kind::Vector(pair.to_vec()),
                        })
                    }
                    Kind::String(units) => units.get(1).map_or_else(nil, |unit| Form {
                        span: 0..0,
                        metadata: vec![],
                        kind: Kind::String(vec![*unit]),
                    }),
                    _ => {
                        return Err(SessionError::Host(wasmtime::Error::msg(
                            "Declared argument lists require seqable reader data",
                        )));
                    }
                };
                fields
                    .retain(|(name, _)| !matches!(*name, "declared" | "fn-var" | "method-params"));
                retained_decl_push!(self, retained, fields; "declared", AnalyzerLiteralKeyword("declared"), self.flag(true)?);
                retained_decl_push!(self, retained, fields; "fn-var", AnalyzerLiteralKeyword("fn-var"), self.flag(true)?);
                retained_decl_push!(self, retained, fields; "method-params", AnalyzerLiteralKeyword("method-params"), self.form(&parameters, depth + 1)?);
            }
            if let Some(tag) = completed_tag {
                let field = if callable.is_some() { "ret-tag" } else { "tag" };
                fields.retain(|(name, _)| *name != field);
                let value = self.form(&tag, depth + 1)?;
                self.charge(2)?;
                if let Some(keys) = retained.as_mut() {
                    let token = if callable.is_some() { AnalyzerLiteralKeyword("ret-tag") }
                        else { AnalyzerLiteralKeyword("tag") };
                    keys.remember_named(self, token)?;
                }
                fields.push((field, value));
            }
        }
        // Share the final metadata value of this captured declaration revision,
        // including a top-fn overlay. Reader occurrences remain independently
        // constructed elsewhere; resolved var copies reuse this catalog value only
        // within this build. Cross-build roots require the checked reuse path.
        let mut metadata_recipe = None;
        if let Some((_, meta)) = fields.iter_mut().find(|(name, _)| *name == "meta") {
            self.charge(0)?;
            *meta = self.recipe(Recipe::Alias(*meta))?;
            metadata_recipe = Some(*meta);
        }
        if self.capture_roots && let Some(revision) = info {
            for (slot, (_, value)) in fields.iter_mut().enumerate() {
                self.session.charge_host_work((global.namespace().len()+global.name().len()) as u64+2)?;
                *value = self.source_boundary(source_roots::Owner::DeclarationField(
                    global.clone(), revision.clone(), slot), *value)?;
            }
        }
        self.charge(2)?;
        let native = if let Some(keys) = retained { keys.finish(self, fields)? }
            else { self.map(fields)? };
        let value = if let Some(revision) = info {
            self.source_boundary(source_roots::Owner::Declaration(global.clone(), revision.clone()), native)?
        } else { native };
        if let (Some(recipe), Some(revision)) = (metadata_recipe, info) {
            // Capture only after the final declaration and overlaid metadata
            // recipe exist. No guest storage is consulted or retained.
            metadata_expectations::capture(self.session, &mut self.metadata_sources,
                global, revision, recipe)?;
        }
        trace(self.session, "finished")?;
        self.declarations.insert(key, (info.cloned(), value));
        Ok(value)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use portable::resolve::{Environment, Phase, VALUE_FIELDS};
    const CONTROLS: &[&str] = &[
        "(def subject 17)",
        "(def ^number subject 17)",
        "(def ^{:tag false :dynamic true} subject 17)",
        "(def ^{:doc raw-doc :meta {:x 1} :fn-var false :variadic? false} subject 17)",
        "(def ^{:doc raw-doc} subject \"computed doc\" 17)",
        "(def subject (fn* [x] x))",
        "(def subject (fn* ([x] x) ([x y & rest] rest)))",
        "(def ^{:arglists '([^{:note 7} x])} subject (fn* [x] x))",
        "(def ^{:top-fn {:variadic? false :max-fixed-arity 7}} subject (fn* [] 17))",
        "(def ^{:variadic? true :max-fixed-arity 7 :arglists [x] :top-fn {}} subject (fn* [] 17))",
        "(def ^{:top-fn [:doc \"top\"]} subject \"doc\" (fn* [] 17))",
        "(def ^{:top-fn {:meta false :doc false :tag nil :ret-tag false :fn-var false}} subject (fn* [] 17))",
        "(def ^{:declared true :arglists '([x])} subject)",
        "(def ^{:declared true :arglists '([x])} subject (fn* [x] x))",
    ];
    fn logical_fields(graph: &mut AnalysisGraph<'_>, root: Value) -> Value {
        let entries = graph.completed_pairs(root).unwrap();
        let mut fields = Vec::new();
        for label in VALUE_FIELDS {
            let value = graph.declaration_field_value(&entries, label).unwrap();
            let present = graph.flag(value.is_some()).unwrap();
            let value = value.unwrap_or_else(|| graph.scalar(Literal::Nil).unwrap());
            let observation = graph.vector(&[present, value]).unwrap();
            fields.push(observation);
        }
        graph.vector(&fields).unwrap()
    }
    // Use the same genuine shipped source catalog as Session::provision_core,
    // rather than fabricating a declaration for the variadic rest class. This
    // test analyzes native source but does not execute the function body.
    fn ordinary_control_environment(phase: Phase, namespace: &str) -> Environment {
        let shipped = portable::bootstrap::shipped(phase)
            .unwrap_or_else(|error| panic!("ordinary control bootstrap {phase:?}: {error:?}"));
        let mut environment = shipped.environment.clone();
        environment.enter_namespace(phase, namespace)
            .unwrap_or_else(|error| panic!("ordinary control namespace {phase:?}/{namespace}: {error:?}"));
        let rest = suss_reader::Symbol {
            namespace: Some("suss.core".into()),
            name: "IndexedSeq".into(),
        };
        let class = environment.resolve(phase, &rest, 0..0)
            .unwrap_or_else(|error| panic!("real shipped rest class {phase:?}: {error:?}"))
            .global().clone();
        assert_eq!(class.phase(), phase);
        assert!(shipped.cells.contains(&class), "rest class must be an actual shipped cell");
        let info = environment.definition_info(&class).expect("real source type declaration");
        assert!(info.analysis_completed);
        assert_eq!(info.type_fields, Some(3), "source IndexedSeq [arr i meta], not a placeholder binding");
        environment
    }
    #[test]
    fn shared_completed_value_writer_matches_original_full_ordinary_projection_cold() {
        for (phase, mut session) in [(Phase::Runtime, Session::new_repl().unwrap()), (Phase::Macro, Session::new_macro().unwrap())] {
            let bridge = FormBridge::new(&mut session).unwrap();
            let baseline = ordinary_control_environment(phase, session.current_namespace());
            for source in CONTROLS {
                let forms = suss_reader::forms::resolve_conditionals(
                    suss_reader::forms::read_forms(source).unwrap()).unwrap();
                let origin = portable::SourceOrigin::new(*source, None);
                // Each unchanged program starts from the same pristine core
                // catalog; previous subject declarations are not accumulated.
                let (_, environment) = hir::prepare_with_origin(&forms, 0..source.len(), &baseline, phase,
                    &mut portable::NoExpansion, Some(&origin))
                    .unwrap_or_else(|error| panic!("ordinary control prepare {phase:?} source={source:?}: {error:?}"));
                let binding = environment.resolve(phase, &suss_reader::Symbol::new("subject"), 0..0).unwrap();
                let global = binding.global();
                let info = environment.shared_definition_info(global).unwrap();
                let mut actuals = Vec::new();
                for route in 0..3 {
                    let actual = {
                        let mut graph = AnalysisGraph::new(&bridge, &mut session);
                        let root = if route == 0 { graph.legacy_declaration(global, Some(info), 0).unwrap() }
                            else if route == 1 { graph.declaration(global, Some(info), 0).unwrap() }
                            else {
                                let owner = info.completed_values.as_ref().unwrap();
                                // Cold direct projection: no ordinary Graph
                                // declaration has populated a completed cache.
                                let values = graph.completed_field_values(global, owner, info, 0).unwrap();
                                graph.map(values).unwrap()
                            };
                        let root = logical_fields(&mut graph, root);
                        graph.materialize(root).unwrap()
                    };
                    session.collect().unwrap();
                    actuals.push(bridge.read(&mut session, &actual, 0..0).unwrap());
                }
                let mut expected = actuals[0].clone();
                if *source == "(def ^number subject 17)" {
                    // Pinned indexing-reader + actual analyzer/&env reference:
                    // raw tag, final :tag and inner :meta :tag share this
                    // located Symbol. The old native scalar path was bare.
                    // Qualified ONLY for this executed reference source; all
                    // other fields/metadata and all other programs stay strict.
                    let pinned = suss_reader::forms::read_forms(
                        "^{:line 1 :column 7 :end-line 1 :end-column 13} number").unwrap().remove(0);
                    let pinned = bridge.quote(&mut session, pinned).unwrap();
                    let pinned = bridge.read(&mut session, &pinned, 0..0).unwrap();
                    let Kind::Vector(fields) = &mut expected.kind else { panic!("all22 fields"); };
                    assert_eq!(fields.len(), VALUE_FIELDS.len());
                    let tag = VALUE_FIELDS.iter().position(|name| *name == "tag").unwrap();
                    let Kind::Vector(pair) = &mut fields[tag].kind else { panic!("tag presence/value"); };
                    assert_eq!(pair.len(), 2);
                    assert_eq!(pair[0].kind, Kind::Bool(true));
                    assert_eq!(pair[1].kind, pinned.kind, "legacy still independently checks exact scalar data");
                    assert!(pair[1].metadata.is_empty(), "retain observed legacy counterexample");
                    assert!(!pinned.metadata.is_empty());
                    pair[1].metadata = pinned.metadata.clone();
                    assert_ne!(actuals[0].kind, expected.kind, "explicit reference-qualified exception, not silent legacy rewrite");
                }
                assert_eq!(expected.kind, actuals[1].kind, "shared ordinary, reference-qualified legacy {phase:?}: {source}");
                assert_eq!(expected.kind, actuals[2].kind, "cold helper, reference-qualified legacy {phase:?}: {source}");
            }
        }
    }
    #[test]
    fn changed_ordinary_revision_declines_retained_owner_and_uses_original_projection() {
        let mut session = Session::new_macro().unwrap();
        let bridge = FormBridge::new(&mut session).unwrap();
        let source = "(def ^number subject 17)";
        let forms = suss_reader::forms::resolve_conditionals(
                    suss_reader::forms::read_forms(source).unwrap()).unwrap();
        let origin = portable::SourceOrigin::new(source, None);
        let environment = hir::prepare_with_origin(&forms, 0..source.len(), &Environment::default(), Phase::Macro,
            &mut portable::NoExpansion, Some(&origin)).unwrap().1;
        let binding = environment.resolve(Phase::Macro, &suss_reader::Symbol::new("subject"), 0..0).unwrap();
        let global = binding.global();
        let info = environment.shared_definition_info(global).unwrap();
        let mut changed = info.as_ref().clone();
        changed.docstring = Some("changed revision".encode_utf16().collect());
        let changed = Arc::new(changed);
        let owner = changed.completed_values.as_ref().unwrap();
        let mut observed = Vec::new();
        for legacy in [true, false] {
            let value = {
                let mut graph = AnalysisGraph::new(&bridge, &mut session);
                assert!(!graph.completed_owner_is_current(global, owner, &changed).unwrap());
                let before = graph.recipes.len();
                assert!(matches!(graph.completed_field_values(global, owner, &changed, 0), Err(SessionError::Host(_))));
                assert_eq!(graph.recipes.len(), before);
                let root = if legacy { graph.legacy_declaration(global, Some(&changed), 0).unwrap() }
                    else { graph.declaration(global, Some(&changed), 0).unwrap() };
                let root = logical_fields(&mut graph, root);
                graph.materialize(root).unwrap()
            };
            session.collect().unwrap();
            observed.push(bridge.read(&mut session, &value, 0..0).unwrap());
        }
        assert_eq!(observed[0].kind, observed[1].kind);
    }

    #[test]
    fn completed_tag_metadata_matches_pinned_reader_modes_without_mutating_native_source() {
        // Actual retained analyzer.js/tools.reader observation, pin c4295f3:
        // pr261-scalar-tag-indexing-reference-1bea-v1. Five bounded rows select
        // the meaningful plain/indexing/file/string/function distinctions.
        let cases = [
            ("(def ^number subject 17)", false, None, "number", false, true),
            ("(def ^number subject 17)", true, None,
             "^{:line 1 :column 7 :end-line 1 :end-column 13} number", false, true),
            ("(def ^\"number\" subject 17)", true, None, "\"number\"", false, false),
            ("(def ^{:tag ^{:marker :kept} number} subject 17)", true, Some("tag-oracle.cljs"),
             "^{:file \"tag-oracle.cljs\" :line 1 :column 30 :end-line 1 :end-column 36 :marker :kept} number", false, true),
            ("(def ^number subject (fn* [] 17))", true, None,
             "^{:line 1 :column 7 :end-line 1 :end-column 13} number", true, true),
        ];
        for (phase, mut session) in [(Phase::Runtime, Session::new_repl().unwrap()), (Phase::Macro, Session::new_macro().unwrap())] {
            let bridge = FormBridge::new(&mut session).unwrap();
            let same = session.eval("(fn [values] (identical? (nth values 0) (nth values 1)))").unwrap();
            for (source, indexed, file, expected, returns, physical_symbol) in cases {
                let forms = suss_reader::forms::read_forms(source).unwrap();
                let origin = indexed.then(|| portable::SourceOrigin::new(source, file.map(std::path::PathBuf::from)));
                let environment = hir::prepare_with_origin(&forms, 0..source.len(), &Environment::default(), phase,
                    &mut portable::NoExpansion, origin.as_ref()).unwrap().1;
                let binding = environment.resolve(phase, &suss_reader::Symbol::new("subject"), 0..0).unwrap();
                let info = environment.shared_definition_info(binding.global()).unwrap();
                let original_source = info.declaration.clone();
                let expected = suss_reader::forms::read_forms(expected).unwrap().remove(0);
                let expected = bridge.quote(&mut session, expected).unwrap();
                session.collect().unwrap();
                let expected = bridge.read(&mut session, &expected, 0..0).unwrap();
                let actual = {
                    let mut graph = AnalysisGraph::new(&bridge, &mut session);
                    let root = graph.declaration(binding.global(), Some(info), 0).unwrap();
                    let fields = graph.completed_pairs(root).unwrap();
                    let tag = graph.declaration_field_value(&fields, "tag").unwrap().expect("outer tag");
                    let meta = graph.declaration_field_value(&fields, "meta").unwrap().expect("meta map");
                    let fields_meta = graph.completed_pairs(meta).unwrap();
                    let inner = graph.declaration_field_value(&fields_meta, "tag").unwrap().expect("inner tag");
                    let ret = graph.declaration_field_value(&fields, "ret-tag").unwrap();
                    assert_eq!(ret.is_some(), returns);
                    let ret = ret.unwrap_or_else(|| graph.scalar(Literal::Nil).unwrap());
                    let root = graph.vector(&[tag, inner, ret]).unwrap();
                    graph.materialize(root).unwrap()
                };
                session.collect().unwrap();
                let data = bridge.read(&mut session, &actual, 0..0).unwrap();
                let Kind::Vector(values) = &data.kind else { panic!("three exact values"); };
                assert_eq!(values.len(), 3);
                assert_eq!(values[0], expected, "outer {phase:?} indexed={indexed} {source}");
                assert_eq!(values[1], expected, "inner {phase:?} indexed={indexed} {source}");
                if returns { assert_eq!(values[2], expected, "return tag retains reader metadata"); }
                else { assert_eq!(values[2].kind, Kind::Nil); }
                if physical_symbol {
                    let identity = session.invoke(&same, &[&actual]).unwrap();
                    assert_eq!(bridge.read(&mut session, &identity, 0..0).unwrap().kind, Kind::Bool(true),
                        "outer/inner Symbol identity {phase:?} indexed={indexed} file={file:?} source={source:?}");
                }
                assert_eq!(info.declaration, original_source, "immutable native source unchanged");
            }
        }
    }

}
