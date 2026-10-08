//! Bounded original bootstrap lowering, not copied upstream macro source.
use super::*;
use crate::portable::resolve::{NominalForm, ProtocolMethod};

impl Analyzer<'_> {
    pub(super) fn nominal(&self, form: &Form, operation: Nominal, arguments: Vec<Hir>) -> Hir {
        Hir {
            source: None,
            span: form.span.clone(),
            metadata: form.metadata.clone(),
            ty: operation.result(),
            kind: Expression::Nominal {
                operation,
                arguments,
            },
        }
    }
    pub(super) fn local(&self, form: &Form, id: BindingId) -> Hir {
        Hir {
            source: None,
            span: form.span.clone(),
            metadata: Vec::new(),
            ty: Type::Value,
            kind: Expression::Local(id),
        }
    }
    pub(super) fn literal_form(&self, form: &Form, literal: Literal) -> Hir {
        Hir {
            source: None,
            span: form.span.clone(),
            metadata: form.metadata.clone(),
            ty: literal.ty(),
            kind: Expression::Literal(literal),
        }
    }
    pub(super) fn fresh_binding(&mut self, form: &Form, value: Hir) -> Binding {
        let id = BindingId(self.next);
        self.next += 1;
        Binding {
            id,
            name: format!("$nominal{}", id.0),
            span: form.span.clone(),
            metadata: Vec::new(),
            value,
        }
    }
    fn nominal_definition(
        &mut self,
        form: &Form,
        name: &Form,
        initializer: Hir,
    ) -> Result<Hir, Diagnostic> {
        let mut definition = self.definition(form, &[name.clone()], false)?;
        let Expression::Definition {
            initializer: target,
            ..
        } = &mut definition.kind
        else {
            unreachable!()
        };
        *target = Some(Box::new(initializer));
        Ok(definition)
    }
    fn stable_key(
        &mut self,
        form: &Form,
        protocol: &Global,
        method: &str,
        arity: usize,
        schema: Vec<Hir>,
    ) -> Hir {
        let global = self.environment.protocol_key(protocol, method, arity);
        let descriptor = self.nominal(form, Nominal::Descriptor, schema);
        let initialize = Hir {
            source: None,
            span: form.span.clone(),
            metadata: Vec::new(),
            ty: Type::Value,
            kind: Expression::Definition {
                global: global.clone(),
                name_metadata: Vec::new(),
                name_span: form.span.clone(),
                docstring: None,
                initializer: Some(Box::new(descriptor)),
                once: true,
            },
        };
        let read = Hir {
            source: None,
            span: form.span.clone(),
            metadata: Vec::new(),
            ty: Type::Value,
            kind: Expression::Global(global),
        };
        self.do_hir(form, vec![initialize, read])
    }
    /// Prepare one source callee before its arguments. Canonical closures keep
    /// the universal ABI path; objects invoke the actual retained IFn method.
    /// The branch is lazy so function calls do not read protocol bindings.
    pub(super) fn prepare_source_callee(
        &mut self,
        form: &Form,
        value: Hir,
        arity: usize,
    ) -> Result<Hir, Diagnostic> {
        if matches!(value.ty, Type::Closure(_)) {
            return Ok(value);
        }
        let protocol = self
            .environment
            .protocols
            .keys()
            .find(|global| {
                global.phase() == self.phase
                    && global.namespace() == "suss.core"
                    && global.name() == "IFn"
            })
            .cloned();
        let Some(protocol) = protocol else {
            // Before compiled core declares IFn, bootstrap calls are closures.
            return Ok(value);
        };
        let callee = self.fresh_binding(form, value);
        let test = self.nominal(form, Nominal::IsClosure, vec![self.local(form, callee.id)]);
        let signature = arity
            .checked_add(1)
            .ok_or_else(|| fail(form.span.clone(), "Too many callable arguments"))?;
        let schema = (0..signature)
            .map(|_| self.literal_form(form, Literal::String(vec![])))
            .collect();
        let key = self.stable_key(form, &protocol, "-invoke", signature, schema);
        // Ordinary emitted ClojureScript calls capture the object's callable
        // method before evaluating arguments. Explicit -invoke separately uses
        // its live native protocol table and receiver convention.
        let bound = self.nominal(
            form,
            Nominal::BindCallable,
            vec![self.local(form, callee.id), key],
        );
        let body = Hir {
            source: None,
            span: form.span.clone(),
            metadata: vec![],
            ty: Type::Value,
            kind: Expression::If {
                condition: Box::new(test),
                consequent: Box::new(self.local(form, callee.id)),
                alternative: Box::new(bound),
            },
        };
        Ok(Hir {
            source: None,
            span: form.span.clone(),
            metadata: vec![],
            ty: Type::Value,
            kind: Expression::Let {
                bindings: vec![callee],
                body: Box::new(body),
            },
        })
    }
    pub(super) fn callable_named_get(&mut self, form: &Form, owner: Hir, name: Hir, spelling: &str) -> Result<Hir, Diagnostic> {
        if !matches!(spelling, "call" | "apply") && !spelling.starts_with("cljs$core$IFn$_invoke$arity$") {
            return Ok(self.nominal(form, Nominal::NamedGet, vec![owner, name]));
        }
        let protocol = self.environment.protocols.keys().find(|global| global.phase() == self.phase && global.namespace() == "suss.core" && global.name() == "IFn").cloned();
        let Some(protocol) = protocol else { return Ok(self.nominal(form, Nominal::NamedGet, vec![owner, name])); };
        let table = self.environment.protocol_key(&protocol, "$callable-property-keys", 22);
        if !self.callable_keys.contains_key(&table) {
            let keys = (1..=22).map(|arity| {
                let schema = (0..arity).map(|_| self.literal_form(form, Literal::String(vec![]))).collect();
                self.stable_key(form, &protocol, "-invoke", arity, schema)
            }).collect();
            let keys = self.nominal(form, Nominal::Array, keys);
            let definition = Hir { source: None, span: form.span.clone(), metadata: vec![], ty: Type::Value,
                kind: Expression::Definition { global: table.clone(), name_metadata: vec![], name_span: form.span.clone(), docstring: None,
                    initializer: Some(Box::new(keys)), once: true } };
            self.callable_keys.insert(table.clone(), definition);
        }
        let keys = Hir { source: None, span: form.span.clone(), metadata: vec![], ty: Type::Value, kind: Expression::Global(table) };
        Ok(self.nominal(form, Nominal::CallableGet, vec![owner, name, keys]))
    }
    fn named_global(&self, name: &Form) -> Result<Global, Diagnostic> {
        let Kind::Symbol(symbol) = &name.kind else {
            return Err(fail(
                name.span.clone(),
                "Expected a resolved type/protocol name",
            ));
        };
        Ok(self
            .environment
            .resolve(self.phase, symbol, name.span.clone())?
            .global()
            .clone())
    }
    fn do_hir(&self, form: &Form, items: Vec<Hir>) -> Hir {
        Hir {
            source: None,
            span: form.span.clone(),
            metadata: form.metadata.clone(),
            ty: items.last().map_or(Type::Nil, |item| item.ty),
            kind: Expression::Do(items),
        }
    }
    pub(super) fn nominal_form(
        &mut self,
        form: &Form,
        args: &[Form],
        operation: NominalForm,
    ) -> Result<Hir, Diagnostic> {
        match operation {
            NominalForm::Instance => {
                if args.len() != 2 {
                    return Err(fail(
                        form.span.clone(),
                        "instance? requires exactly two operands",
                    ));
                }
                let arguments = args
                    .iter()
                    .map(|argument| self.form(argument))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(self.nominal(form, Nominal::Instance, arguments))
            }
            NominalForm::Implements => {
                if args.len() != 2 {
                    return Err(fail(
                        form.span.clone(),
                        "implements? requires a protocol name and value",
                    ));
                }
                // The pin resolves the protocol syntactically and tests direct
                // implementation only. Native/default fallback is satisfies?.
                let protocol = self.named_global(&args[0])?;
                let marker = self.stable_key(&args[0], &protocol, "", 0, vec![]);
                let value = self.form(&args[1])?;
                Ok(self.nominal(form, Nominal::Satisfies, vec![marker, value]))
            }
            NominalForm::Satisfies => {
                if args.len() != 2 {
                    return Err(fail(
                        form.span.clone(),
                        "satisfies? requires a protocol name and value",
                    ));
                }
                let protocol = self.named_global(&args[0])?;
                // Like the pin's macro, the marker follows the resolved syntactic
                // protocol name, not a runtime alias of a protocol object.
                let marker = self.stable_key(&args[0], &protocol, "", 0, vec![]);
                let value = self.form(&args[1])?;
                let binding = self.fresh_binding(&args[1], value);
                let value = self.local(&args[1], binding.id);
                let condition = self.nominal(form, Nominal::Satisfies, vec![marker, value.clone()]);
                let protocol_value = Hir {
                    source: None,
                    span: args[0].span.clone(),
                    metadata: args[0].metadata.clone(),
                    ty: Type::Value,
                    kind: Expression::Global(protocol),
                };
                let symbol = suss_reader::Symbol {
                    namespace: Some("suss.core".into()),
                    name: "native-satisfies?".into(),
                };
                let (callee, ty) = self.global_value(&symbol, form.span.clone())?;
                let alternative = Hir {
                    source: None,
                    span: form.span.clone(),
                    metadata: Vec::new(),
                    ty: Type::Value,
                    kind: Expression::Call {
                        callee: Box::new(Hir {
                            source: None,
                            span: form.span.clone(),
                            metadata: Vec::new(),
                            ty,
                            kind: callee,
                        }),
                        arguments: vec![protocol_value, value],
                    },
                };
                let body = Hir {
                    source: None,
                    span: form.span.clone(),
                    metadata: form.metadata.clone(),
                    ty: Type::Value,
                    kind: Expression::If {
                        condition: Box::new(condition),
                        consequent: Box::new(self.literal_form(form, Literal::Bool(true))),
                        alternative: Box::new(alternative),
                    },
                };
                Ok(Hir {
                    source: None,
                    span: form.span.clone(),
                    metadata: form.metadata.clone(),
                    ty: Type::Value,
                    kind: Expression::Let {
                        bindings: vec![binding],
                        body: Box::new(body),
                    },
                })
            }
            NominalForm::Deftype => self.type_definition(form, args),
            NominalForm::Defprotocol => self.protocol_definition(form, args),
            NominalForm::ExtendType => {
                let Some(class) = args.first() else {
                    return Err(fail(form.span.clone(), "extend-type requires a type"));
                };
                if let Some(kind) = NativeKind::from_form(class) {
                    let mut effects = self.native_protocol_extensions(form, &args[1..], kind)?;
                    if effects.is_empty() {
                        effects.push(self.literal_form(form, Literal::Nil));
                    }
                    return Ok(self.do_hir(form, effects));
                }
                if matches!(&class.kind, Kind::Symbol(symbol) if symbol.namespace.is_none() && matches!(symbol.name.as_str(), "symbol" | "bigint"))
                {
                    return Err(fail(
                        class.span.clone(),
                        "Native symbol/bigint values are not lowered yet",
                    ));
                }
                let value = self.form(class)?;
                let binding = self.fresh_binding(class, value);
                let class_value = self.local(class, binding.id);
                let mut effects =
                    self.protocol_extensions(form, &args[1..], class_value, &[], false, class)?;
                if effects.is_empty() {
                    effects.push(self.literal_form(form, Literal::Nil));
                }
                let body = self.do_hir(form, effects);
                Ok(Hir {
                    source: None,
                    span: form.span.clone(),
                    metadata: form.metadata.clone(),
                    ty: body.ty,
                    kind: Expression::Let {
                        bindings: vec![binding],
                        body: Box::new(body),
                    },
                })
            }
        }
    }
    // Pinned analyzer retains these three field flags; type hints do not impose
    // a runtime guard. Other field attributes remain outside this bounded port.
    pub(super) fn field_mutable(field: &Form) -> Result<bool, Diagnostic> {
        let mut flags = [false; 3];
        for metadata in field.metadata.iter().rev() {
            let mut apply = |key: &Form, value: bool| -> Result<(), Diagnostic> {
                let Kind::Keyword(key) = &key.kind else {
                    return Err(fail(
                        metadata.span.clone(),
                        "Unsupported type field attribute",
                    ));
                };
                if key.namespace.is_some() {
                    return Err(fail(
                        metadata.span.clone(),
                        "Unsupported type field attribute",
                    ));
                }
                match key.name.as_str() {
                    "mutable" => flags[0] = value,
                    "unsynchronized-mutable" => flags[1] = value,
                    "volatile-mutable" => flags[2] = value,
                    // Compiled macros return indexing-reader metadata on field
                    // symbols. These source facts do not change mutability;
                    // keep them on the declaration for source analysis.
                    "tag" | "file" | "line" | "column" | "end-line" | "end-column" => {}
                    _ => {
                        return Err(fail(
                            metadata.span.clone(),
                            "Unsupported type field attribute",
                        ));
                    }
                }
                Ok(())
            };
            match &metadata.kind {
                Kind::Keyword(_) => apply(metadata, true)?,
                Kind::Symbol(_) | Kind::String(_) => {} // shorthand type tag
                Kind::Map(entries) => {
                    for entry in entries.chunks_exact(2) {
                        apply(
                            &entry[0],
                            !matches!(entry[1].kind, Kind::Nil | Kind::Bool(false)),
                        )?;
                    }
                }
                _ => {
                    return Err(fail(
                        metadata.span.clone(),
                        "Unsupported type field attribute",
                    ));
                }
            }
        }
        Ok(flags.into_iter().any(|flag| flag))
    }
    fn type_definition(&mut self, form: &Form, args: &[Form]) -> Result<Hir, Diagnostic> {
        if args.len() < 2 {
            return Err(fail(
                form.span.clone(),
                "deftype requires a name and field vector",
            ));
        }
        let Kind::Vector(fields) = &args[1].kind else {
            return Err(fail(
                args[1].span.clone(),
                "deftype requires a field vector",
            ));
        };
        let mut seen = BTreeSet::new();
        let mut schema = Vec::new();
        for field in fields {
            let Kind::Symbol(symbol) = &field.kind else {
                return Err(fail(
                    field.span.clone(),
                    "Type fields require unqualified symbols",
                ));
            };
            // Pinned constructors assign this.__proto__, invoking the inherited
            // setter rather than creating a field. Physical protocol slots must
            // not silently turn that prototype mutation into an ordinary field.
            if symbol.name == "__proto__" {
                return Err(fail(
                    field.span.clone(),
                    "Type __proto__ fields require unsupported prototype mutation",
                ));
            }
            if symbol.namespace.is_some() || symbol.name == "&" || !seen.insert(symbol.name.clone())
            {
                return Err(fail(
                    field.span.clone(),
                    "Type field names must be unqualified and distinct",
                ));
            }
            Self::field_mutable(field)?;
            schema.push(
                self.literal_form(field, Literal::String(symbol.name.encode_utf16().collect())),
            );
        }
        // parse-type preserves prior declaration fields independently of the
        // ordinary def/:declared rule. Capture that revision before def can
        // replace it, selecting only this namespace's actual declaration.
        let previous_type_info = if let Kind::Symbol(name) = &args[0].kind {
            self.environment.namespace_scope(self.phase).declarations.into_iter()
                .find(|(global, _)| global.name() == name.name)
                .map(|(_, info)| info.clone())
        } else { None };
        // Declare the source identity before method analysis, for self type references.
        let declaration = self.definition(form, &[args[0].clone()], false)?;
        let Expression::Definition {
            global: class_global,
            ..
        } = &declaration.kind
        else {
            unreachable!()
        };
        let class_global = class_global.clone();
        // Pinned parse-type (analyzer.cljc3614–3649) publishes constructor
        // source facts before analyzing methods. Do not infer these from the
        // later native descriptor/closure storage.
        // A truthy :declared ordinary def can preserve an older revision.
        // A source type has its own current declaration and must not recover
        // its tag/metadata/origin from that ordinary-def preservation rule.
        let mut type_info = previous_type_info.unwrap_or_else(|| {
            self.environment.definition_info(&class_global)
                .expect("declared source type").clone()
        });
        let previous = reader_metadata_pairs(&type_info.declaration)?;
        let mut metadata = previous.chunks_exact(2).filter(|pair| {
            // parse-type replaces these defaults before applying current
            // metadata; unrelated preserved declaration fields stay intact.
            !matches!(&pair[0].kind, Kind::Keyword(key) if key.namespace.is_none()
                && matches!(key.name.as_str(), "tag" | "type" | "num-fields" | "record"))
        }).flat_map(|pair| pair.iter().cloned()).collect::<Vec<_>>();
        for pair in reader_metadata_pairs(&args[0])?.chunks_exact(2) {
            let mut retained = Vec::new();
            for old in metadata.chunks_exact(2) {
                if old[0].kind != pair[0].kind { retained.extend_from_slice(old); }
            }
            retained.extend_from_slice(pair);
            metadata = retained;
        }
        type_info.declaration = args[0].clone();
        type_info.declaration.metadata = if metadata.is_empty() { vec![] } else {
            vec![Form { kind: Kind::Map(metadata), span: 0..0, metadata: vec![] }]
        };
        type_info.definition_form = form.clone();
        type_info.origin = self.origin.clone();
        type_info.type_fields = Some(fields.len());
        type_info.analysis_completed = true;
        self.environment.record_definition(class_global.clone(), type_info);
        let Kind::Symbol(symbol) = &args[0].kind else {
            unreachable!()
        };
        let arrow_name = Form {
            span: args[0].span.clone(),
            metadata: Vec::new(),
            kind: Kind::Symbol(suss_reader::Symbol {
                namespace: symbol.namespace.clone(),
                name: format!("->{}", symbol.name),
            }),
        };
        // Own generated constructor references are available during method analysis.
        self.definition(form, &[arrow_name.clone()], false)?;
        let descriptor = self.nominal(form, Nominal::Descriptor, schema);
        let class = self.nominal(form, Nominal::Class, vec![descriptor]);
        let binding = self.fresh_binding(form, class);
        let class_value = self.local(form, binding.id);
        // Pinned analyzer.cljc parse-type3614–3649 replaces enclosing locals;
        // deftype* methods use namespace globals and their own fields/parameters.
        // extend-type is a runtime expression and keeps its enclosing captures.
        let outer_locals = std::mem::take(&mut self.locals);
        let outer_fields = std::mem::take(&mut self.fields);
        let extensions =
            self.protocol_extensions(form, &args[2..], class_value.clone(), fields, true, &args[0]);
        self.locals = outer_locals;
        self.fields = outer_fields;
        let mut effects = extensions?;
        effects.push(class_value);
        let body = self.do_hir(form, effects);
        let initialization = Hir {
            source: None,
            span: form.span.clone(),
            metadata: Vec::new(),
            ty: Type::Value,
            kind: Expression::Let {
                bindings: vec![binding],
                body: Box::new(body),
            },
        };
        let class_binding = self.fresh_binding(form, initialization);
        let class_value = self.local(form, class_binding.id);
        let mut definition = declaration;
        let Expression::Definition {
            initializer: target,
            ..
        } = &mut definition.kind
        else {
            unreachable!()
        };
        *target = Some(Box::new(class_value.clone()));
        let mut parameters = Vec::new();
        let mut arguments = vec![Hir {
            source: None,
            span: args[0].span.clone(),
            metadata: Vec::new(),
            ty: Type::Value,
            kind: Expression::Global(class_global),
        }];
        for field in fields {
            let Kind::Symbol(symbol) = &field.kind else {
                unreachable!()
            };
            let id = BindingId(self.next);
            self.next += 1;
            parameters.push(Parameter {
                id,
                name: symbol.name.clone(),
                metadata: field.metadata.clone(),
                span: field.span.clone(),
            });
            arguments.push(self.local(field, id));
        }
        let arrow = Hir {
            source: None,
            span: form.span.clone(),
            metadata: Vec::new(),
            ty: Type::Closure(parameters.len()),
            kind: Expression::Function {
                parameters,
                captures: vec![],
                body: Box::new(self.nominal(form, Nominal::Construct, arguments)),
            },
        };
        let arrow = self.nominal_definition(form, &arrow_name, arrow)?;
        let body = self.do_hir(form, vec![definition, arrow, class_value]);
        Ok(Hir {
            source: None,
            span: form.span.clone(),
            metadata: form.metadata.clone(),
            ty: Type::Value,
            kind: Expression::Let {
                bindings: vec![class_binding],
                body: Box::new(body),
            },
        })
    }
    fn protocol_definition(&mut self, form: &Form, args: &[Form]) -> Result<Hir, Diagnostic> {
        let Some(name) = args.first() else {
            return Err(fail(form.span.clone(), "defprotocol requires a name"));
        };
        let declaration = self.definition(form, &[name.clone()], false)?;
        let Expression::Definition {
            global: protocol, ..
        } = &declaration.kind
        else {
            unreachable!()
        };
        let mut bindings = Vec::new();
        let marker = self.stable_key(form, protocol, "", 0, vec![]);
        bindings.push(self.fresh_binding(form, marker));
        let mut methods = Vec::new();
        let mut definitions = Vec::new();
        let mut method_names = BTreeSet::new();
        let mut declarations = &args[1..];
        if declarations
            .first()
            .is_some_and(|arg| matches!(arg.kind, Kind::String(_)))
        {
            declarations = &declarations[1..];
        }
        for declaration in declarations {
            let Kind::List(items) = &declaration.kind else {
                return Err(fail(
                    declaration.span.clone(),
                    "Protocol method declaration requires a list",
                ));
            };
            let Some(method_name) = items.first() else {
                return Err(fail(
                    declaration.span.clone(),
                    "Empty protocol method declaration",
                ));
            };
            let Kind::Symbol(symbol) = &method_name.kind else {
                return Err(fail(
                    method_name.span.clone(),
                    "Protocol method name requires a symbol",
                ));
            };
            if symbol.namespace.is_some() || !method_names.insert(symbol.name.clone()) {
                return Err(fail(
                    method_name.span.clone(),
                    "Protocol method names must be unqualified and distinct",
                ));
            }
            let mut signatures = Vec::new();
            let mut wrappers = Vec::new();
            let mut captures = Vec::new();
            let mut arities = BTreeSet::new();
            for params in &items[1..] {
                if matches!(params.kind, Kind::String(_)) {
                    continue;
                }
                let Kind::Vector(parameters) = &params.kind else {
                    return Err(fail(
                        params.span.clone(),
                        "Protocol signature requires a fixed parameter vector",
                    ));
                };
                if parameters.is_empty() || !arities.insert(parameters.len()) {
                    return Err(fail(
                        params.span.clone(),
                        "Protocol signatures require a receiver and distinct arities",
                    ));
                }
                let mut names = BTreeSet::new();
                let mut schema = Vec::new();
                for parameter in parameters {
                    let Kind::Symbol(symbol) = &parameter.kind else {
                        return Err(fail(
                            parameter.span.clone(),
                            "Protocol parameters require symbols",
                        ));
                    };
                    if symbol.namespace.is_some()
                        || symbol.name == "&"
                        || !names.insert(symbol.name.clone())
                    {
                        return Err(fail(
                            parameter.span.clone(),
                            "Protocol parameters must be unqualified fixed names",
                        ));
                    }
                    schema.push(self.literal_form(
                        parameter,
                        Literal::String(symbol.name.encode_utf16().collect()),
                    ));
                }
                let key_index = bindings.len();
                let descriptor =
                    self.stable_key(params, protocol, &symbol.name, parameters.len(), schema);
                let key = self.fresh_binding(params, descriptor);
                let key_value = self.local(params, key.id);
                bindings.push(key);
                let namespace = self.environment.current_namespace(self.phase).to_owned();
                let method_global =
                    self.environment
                        .declare_cell(self.phase, &namespace, &symbol.name)?;
                let cell = Hir {
                    source: None,
                    span: method_name.span.clone(),
                    metadata: Vec::new(),
                    ty: Type::Value,
                    kind: Expression::GlobalCell(method_global),
                };
                // IFn's emitted JS methods omit the physical receiver formal.
                // Explicit -invoke still passes the target in its argument list,
                // so nominal dispatch selects the next source method arity.
                let dispatcher = if protocol.namespace() == "suss.core"
                    && protocol.name() == "IFn"
                    && symbol.name == "-invoke"
                {
                    let schema = (0..=parameters.len())
                        .map(|_| self.literal_form(params, Literal::String(vec![])))
                        .collect();
                    let next = self.stable_key(params, protocol, &symbol.name, parameters.len() + 1, schema);
                    let next = self.fresh_binding(params, next);
                    let next_value = self.local(params, next.id);
                    bindings.push(next);
                    self.nominal(
                        params,
                        Nominal::IFnLiveDispatcher,
                        vec![key_value, cell, next_value],
                    )
                } else {
                    self.nominal(params, Nominal::LiveDispatcher, vec![key_value, cell])
                };
                let dispatcher = self.fresh_binding(params, dispatcher);
                captures.push(dispatcher.id);
                let callee = self.local(params, dispatcher.id);
                // Dispatcher locals are not part of the public protocol bundle.
                let mut wrapper_params = Vec::new();
                let mut arguments = Vec::new();
                for parameter in parameters {
                    let Kind::Symbol(symbol) = &parameter.kind else {
                        unreachable!()
                    };
                    let id = BindingId(self.next);
                    self.next += 1;
                    wrapper_params.push(Parameter {
                        id,
                        name: symbol.name.clone(),
                        metadata: parameter.metadata.clone(),
                        span: parameter.span.clone(),
                    });
                    arguments.push(self.local(parameter, id));
                }
                wrappers.push(Method {
                    variadic: false,
                    parameters: wrapper_params,
                    body: Box::new(Hir {
                        source: None,
                        span: declaration.span.clone(),
                        metadata: Vec::new(),
                        ty: Type::Value,
                        kind: Expression::Call {
                            callee: Box::new(callee),
                            arguments,
                        },
                    }),
                });
                definitions.push((method_name.clone(), dispatcher));
                signatures.push((parameters.len(), key_index));
            }
            if signatures.is_empty() {
                return Err(fail(
                    declaration.span.clone(),
                    "Protocol method requires a signature",
                ));
            }
            let function = Hir {
                source: None,
                span: declaration.span.clone(),
                metadata: declaration.metadata.clone(),
                ty: Type::Value,
                kind: Expression::GeneralFunction {
                    methods: wrappers,
                    captures,
                    self_binding: None,
                rest_class: None,
                },
            };
            methods.push(ProtocolMethod {
                name: symbol.name.clone(),
                signatures,
            });
            // Store each complete method definition after its dispatcher bindings.
            definitions.push((
                method_name.clone(),
                self.fresh_binding(declaration, function),
            ));
        }
        self.environment.protocols.insert(protocol.clone(), methods);
        let bundle = self.nominal(
            form,
            Nominal::Array,
            bindings
                .iter()
                .map(|binding| self.local(form, binding.id))
                .collect(),
        );
        let protocol_value = self.nominal(form, Nominal::Protocol, vec![bundle]);
        let mut effects = vec![self.nominal_definition(form, name, protocol_value)?];
        // Only the last entry for each method is the complete wrapper; preceding
        // entries are the captured dispatcher bindings, initialized in order.
        for (method_name, binding) in definitions {
            if matches!(binding.value.kind, Expression::GeneralFunction { .. }) {
                let value = self.local(&method_name, binding.id);
                bindings.push(binding);
                effects.push(self.nominal_definition(form, &method_name, value)?);
            } else {
                bindings.push(binding);
            }
        }
        // The pin's trailing compiler-only unchecked-if set! emits undefined,
        // which is nil-like to the encoder but has distinct coercions.
        effects.push(self.literal_form(form, Literal::Undefined));
        let body = self.do_hir(form, effects);
        Ok(Hir {
            source: None,
            span: form.span.clone(),
            metadata: form.metadata.clone(),
            ty: Type::Value,
            kind: Expression::Let {
                bindings,
                body: Box::new(body),
            },
        })
    }
    fn native_protocol_extensions(
        &mut self,
        form: &Form,
        args: &[Form],
        kind: NativeKind,
    ) -> Result<Vec<Hir>, Diagnostic> {
        let mut effects = Vec::new();
        let mut index = 0;
        while index < args.len() {
            let protocol_name = &args[index];
            index += 1;
            let global = self.named_global(protocol_name)?;
            let methods = self
                .environment
                .protocols
                .get(&global)
                .cloned()
                .ok_or_else(|| {
                    fail(
                        protocol_name.span.clone(),
                        "Extension requires a declared protocol",
                    )
                })?;
            let protocol = Hir {
                source: None,
                span: protocol_name.span.clone(),
                metadata: protocol_name.metadata.clone(),
                ty: Type::Value,
                kind: Expression::Global(global.clone()),
            };
            effects.push(self.nominal(protocol_name, Nominal::NativeMarker(kind), vec![protocol]));
            while index < args.len() && matches!(args[index].kind, Kind::List(_)) {
                let method_form = &args[index];
                index += 1;
                let Kind::List(items) = &method_form.kind else {
                    unreachable!()
                };
                let Some(method_name) = items.first() else {
                    return Err(fail(method_form.span.clone(), "Empty extension method"));
                };
                let Kind::Symbol(symbol) = &method_name.kind else {
                    return Err(fail(
                        method_name.span.clone(),
                        "Extension method requires a name",
                    ));
                };
                let method = methods
                    .iter()
                    .find(|method| method.name == symbol.name && symbol.namespace.is_none())
                    .ok_or_else(|| {
                        fail(
                            method_name.span.clone(),
                            "Method is not declared by this protocol",
                        )
                    })?;
                let signatures = if items
                    .get(1)
                    .is_some_and(|item| matches!(item.kind, Kind::Vector(_)))
                {
                    vec![&items[1..]]
                } else {
                    items[1..]
                        .iter()
                        .map(|item| {
                            if let Kind::List(items) = &item.kind {
                                Ok(items.as_slice())
                            } else {
                                Err(fail(
                                    item.span.clone(),
                                    "Extension signature requires a list",
                                ))
                            }
                        })
                        .collect::<Result<Vec<_>, _>>()?
                };
                if signatures.is_empty() {
                    return Err(fail(
                        method_form.span.clone(),
                        "Extension method requires a signature",
                    ));
                }
                for signature in &signatures {
                    let Some(params) = signature.first() else {
                        return Err(fail(method_form.span.clone(), "Empty extension signature"));
                    };
                    let Kind::Vector(parameters) = &params.kind else {
                        return Err(fail(
                            params.span.clone(),
                            "Extension signature requires a parameter vector",
                        ));
                    };
                    if !method
                        .signatures
                        .iter()
                        .any(|(arity, _)| *arity == parameters.len())
                    {
                        return Err(fail(
                            params.span.clone(),
                            "Extension arity is not declared by this protocol",
                        ));
                    }
                }
                // Base-type assignment replaces one entire multi-arity function,
                // unlike independent prototype slots on a nominal extension.
                let operator = Form {
                    span: method_name.span.clone(),
                    metadata: Vec::new(),
                    kind: Kind::Symbol(suss_reader::Symbol {
                        namespace: None,
                        name: "fn*".into(),
                    }),
                };
                let mut function_items = vec![operator];
                function_items.extend_from_slice(&items[1..]);
                let function_form = Form {
                    span: method_form.span.clone(),
                    metadata: method_form.metadata.clone(),
                    kind: Kind::List(function_items),
                };
                let implementation = self.form(&function_form)?;
                let method_symbol = suss_reader::Symbol {
                    namespace: Some(global.namespace().into()),
                    name: method.name.clone(),
                };
                let (method_value, ty) =
                    self.global_value(&method_symbol, method_name.span.clone())?;
                let method_value = Hir {
                    source: None,
                    span: method_name.span.clone(),
                    metadata: Vec::new(),
                    ty,
                    kind: method_value,
                };
                effects.push(self.nominal(
                    form,
                    Nominal::NativeSet(kind),
                    vec![method_value, implementation],
                ));
            }
        }
        Ok(effects)
    }
    fn protocol_extensions(
        &mut self,
        form: &Form,
        args: &[Form],
        class: Hir,
        fields: &[Form],
        in_deftype: bool,
        type_declaration: &Form,
    ) -> Result<Vec<Hir>, Diagnostic> {
        let mut effects = Vec::new();
        let mut index = 0;
        while index < args.len() {
            let protocol_name = &args[index];
            index += 1;
            if matches!(&protocol_name.kind, Kind::Symbol(symbol) if symbol.namespace.is_none() && symbol.name == "Object")
            {
                let mut groups: Vec<(Form, String, Vec<Method>)> = Vec::new();
                while index < args.len() && matches!(args[index].kind, Kind::List(_)) {
                    let method_form = &args[index];
                    index += 1;
                    let Kind::List(items) = &method_form.kind else {
                        unreachable!()
                    };
                    let Some(method_name) = items.first() else {
                        return Err(fail(method_form.span.clone(), "Empty Object method"));
                    };
                    let Kind::Symbol(symbol) = &method_name.kind else {
                        return Err(fail(
                            method_name.span.clone(),
                            "Object method requires a name",
                        ));
                    };
                    if symbol.namespace.is_some() {
                        return Err(fail(
                            method_name.span.clone(),
                            "Object method requires an unqualified name",
                        ));
                    }
                    let name = Self::property_name(method_name, &format!(".-{}", symbol.name))?;
                    // Pinned add-obj-methods assigns prototype.__proto__; its
                    // inherited setter changes the prototype instead of storing
                    // an ordinary method. The prototype adapter is unfinished.
                    if name == "__proto__" {
                        return Err(fail(
                            method_name.span.clone(),
                            "Object __proto__ methods require unsupported prototype mutation",
                        ));
                    }
                    let group =
                        if let Some(group) = groups.iter().position(|(_, key, _)| key == &name) {
                            group
                        } else {
                            groups.push((method_name.clone(), name, Vec::new()));
                            groups.len() - 1
                        };
                    let signatures = if items
                        .get(1)
                        .is_some_and(|item| matches!(item.kind, Kind::Vector(_)))
                    {
                        vec![&items[1..]]
                    } else {
                        if in_deftype {
                            return Err(fail(
                                method_form.span.clone(),
                                "deftype overloads require separate method forms with parameter vectors",
                            ));
                        }
                        let mut signatures = Vec::new();
                        for signature in &items[1..] {
                            let Kind::List(items) = &signature.kind else {
                                return Err(fail(
                                    signature.span.clone(),
                                    "Object extension signature requires a list",
                                ));
                            };
                            signatures.push(items.as_slice());
                        }
                        if signatures.is_empty() {
                            return Err(fail(
                                method_form.span.clone(),
                                "Object method requires a signature",
                            ));
                        }
                        signatures
                    };
                    for signature in signatures {
                        // Pinned core.cljc adapt-obj-params anchors this-as outside
                        // the loop, with only user parameters replaced by recur.
                        let (implementation, _) = self.fixed_function_fields(
                            method_form,
                            signature,
                            true,
                            fields,
                            true,
                            true,
                            None,
                            Some(type_declaration),
                        )?;
                        let Expression::Function {
                            parameters, body, ..
                        } = implementation.kind
                        else {
                            unreachable!()
                        };
                        groups[group].2.push(Method { parameters, body, variadic: false });
                    }
                }
                for (method_name, name, mut methods) in groups {
                    methods.reverse();
                    let mut arities = BTreeSet::new();
                    methods.retain(|method| arities.insert(method.parameters.len()));
                    methods.reverse();
                    let mut captures = BTreeSet::new();
                    for method in &methods {
                        let bound = method
                            .parameters
                            .iter()
                            .map(|parameter| parameter.id)
                            .collect();
                        free_bindings(&method.body, &bound, &mut captures);
                    }
                    let kind = if methods.len() == 1 {
                        let method = methods.remove(0);
                        Expression::Function {
                            parameters: method.parameters,
                            body: method.body,
                            captures: captures.into_iter().collect(),
                        }
                    } else {
                        Expression::GeneralFunction {
                            methods,
                            captures: captures.into_iter().collect(),
                            self_binding: None,
                rest_class: None,
                        }
                    };
                    let implementation = Hir {
                        source: None,
                        span: method_name.span.clone(),
                        metadata: method_name.metadata.clone(),
                        ty: Type::Value,
                        kind,
                    };
                    let key = self
                        .literal_form(&method_name, Literal::String(name.encode_utf16().collect()));
                    effects.push(self.nominal(
                        &method_name,
                        Nominal::ObjectSet,
                        vec![class.clone(), key, implementation],
                    ));
                }
                continue;
            }
            let global = self.named_global(protocol_name)?;
            let methods = self
                .environment
                .protocols
                .get(&global)
                .cloned()
                .ok_or_else(|| {
                    fail(
                        protocol_name.span.clone(),
                        "Extension requires a declared protocol",
                    )
                })?;
            let protocol = Hir {
                source: None,
                span: protocol_name.span.clone(),
                metadata: protocol_name.metadata.clone(),
                ty: Type::Value,
                kind: Expression::Global(global),
            };
            let marker = self.nominal(protocol_name, Nominal::Key(0), vec![protocol.clone()]);
            effects.push(self.nominal(protocol_name, Nominal::Marker, vec![class.clone(), marker]));
            while index < args.len() && matches!(args[index].kind, Kind::List(_)) {
                let method_form = &args[index];
                index += 1;
                let Kind::List(items) = &method_form.kind else {
                    unreachable!()
                };
                let Some(method_name) = items.first() else {
                    return Err(fail(method_form.span.clone(), "Empty extension method"));
                };
                let Kind::Symbol(symbol) = &method_name.kind else {
                    return Err(fail(
                        method_name.span.clone(),
                        "Extension method requires a name",
                    ));
                };
                let method = methods
                    .iter()
                    .find(|method| method.name == symbol.name && symbol.namespace.is_none())
                    .ok_or_else(|| {
                        fail(
                            method_name.span.clone(),
                            "Method is not declared by this protocol",
                        )
                    })?;
                let mut signatures = Vec::new();
                if items
                    .get(1)
                    .is_some_and(|item| matches!(item.kind, Kind::Vector(_)))
                {
                    signatures.push(&items[1..]);
                } else {
                    if in_deftype {
                        return Err(fail(
                            method_form.span.clone(),
                            "deftype overloads require separate method forms with parameter vectors",
                        ));
                    }
                    for signature in &items[1..] {
                        let Kind::List(signature) = &signature.kind else {
                            return Err(fail(
                                method_form.span.clone(),
                                "Extension signature requires a list",
                            ));
                        };
                        signatures.push(signature.as_slice());
                    }
                }
                if signatures.is_empty() {
                    return Err(fail(
                        method_form.span.clone(),
                        "Extension method requires a signature",
                    ));
                }
                for signature in signatures {
                    let Some(params) = signature.first() else {
                        return Err(fail(method_form.span.clone(), "Empty extension signature"));
                    };
                    let Kind::Vector(parameters) = &params.kind else {
                        return Err(fail(
                            params.span.clone(),
                            "Extension signature requires a parameter vector",
                        ));
                    };
                    let key_index = method
                        .signatures
                        .iter()
                        .find(|(arity, _)| *arity == parameters.len())
                        .map(|(_, key)| *key)
                        .ok_or_else(|| {
                            fail(
                                params.span.clone(),
                                "Extension arity is not declared by this protocol",
                            )
                        })?;
                    let (implementation, _) = self.fixed_function_fields(
                        method_form,
                        signature,
                        true,
                        fields,
                        true,
                        false,
                        None,
                        Some(type_declaration),
                    )?;
                    let key =
                        self.nominal(method_name, Nominal::Key(key_index), vec![protocol.clone()]);
                    effects.push(self.nominal(
                        method_form,
                        Nominal::Set,
                        vec![class.clone(), key, implementation],
                    ));
                }
            }
        }
        Ok(effects)
    }
}
