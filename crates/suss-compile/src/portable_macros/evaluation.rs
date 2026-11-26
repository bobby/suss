//! Clojure Compiler.eval's top-level compilation boundary. Each executable
//! unit still goes through the ordinary analyzer/emitter; this does not walk
//! expression bodies or evaluate language operators on the host.
use super::*;
use crate::portable_session::CompilationSnapshot;

pub(crate) enum PreparedEvaluation {
    Fragment(crate::portable::modules::PreparedInput),
    TopLevelDo(Vec<Form>),
    MacroDeclaration(Form),
}

pub(crate) fn prepare_with_expander(
    snapshot: &CompilationSnapshot,
    mut form: Form,
    expander: &mut dyn ExpansionHost,
) -> Result<PreparedEvaluation, SessionError> {
        let mut environment = snapshot.environment.clone();
        let span = form.span.clone();
        let origin = crate::portable::SourceOrigin::native_evaluation(
            snapshot.native_owner.ok_or_else(|| wasmtime::Error::msg("Missing native compilation owner"))?
        );
        {
            let mut request = crate::portable::ExpansionRequest::new(expander, span.clone())
                .map_err(SessionError::Compile)?;
            request
                .charge_expansion_work(
                    environment
                        .current_namespace(snapshot.phase())
                        .len() as u64
                        + 1,
                    span.clone(),
                )
                .map_err(SessionError::Compile)?;
            let namespace = Arc::new(crate::portable::hir::SourceNamespace::capture(
                &environment,
                snapshot.phase(),
            ));
            let locals = Default::default();
            let fields = Default::default();
            // Expand only the root, as Compiler.eval does before deciding its
            // do boundary. Ordinary lexical bodies retain whole-form analysis.
            loop {
                request
                    .charge_expansion_work(1, form.span.clone())
                    .map_err(SessionError::Compile)?;
                let context = ExpansionContext {
                    environment: &environment,
                    namespace_snapshot: &namespace,
                    origin: Some(&origin),
                    phase: snapshot.phase(),
                    context: crate::portable::AnalysisContext::Expression,
                    locals: &locals,
                    fields: &fields,
                    function_scopes: &[],
                };
                let expanded = request.expand(&form, context).map_err(SessionError::Compile)?;
                request.refresh_native_environment(&mut environment, form.span.clone()).map_err(SessionError::Compile)?;
                match expanded {
                    Some(expanded) => form = expanded,
                    None => break,
                }
            }
        }
        if matches!(&form.kind, Kind::List(items) if matches!(items.first().map(|head| &head.kind),
            Some(Kind::Symbol(name)) if environment.resolves_bootstrap_name(snapshot.phase(), name, "defmacro"))) {
            // Normalize aliases only after resolution against the caller's real
            // environment. The existing declaration converter validates the rest.
            let Kind::List(items) = &mut form.kind else { unreachable!() };
            items[0].kind = Kind::Symbol(suss_reader::Symbol::namespaced("suss.core", "defmacro"));
            return Ok(PreparedEvaluation::MacroDeclaration(form));
        }
        if matches!(&form.kind, Kind::List(items) if matches!(items.first().map(|head| &head.kind),
            Some(Kind::Symbol(name)) if name.namespace.is_none() && name.name == "do"))
        {
            let (span, _, kind) = form.into_parts();
            let Kind::List(mut forms) = kind else {
                unreachable!()
            };
            forms.remove(0);
            if forms.is_empty() {
                // Even the empty do goes through the same compiler pipeline.
                forms.push(Form {
                    span,
                    metadata: vec![],
                    kind: Kind::Nil,
                });
            }
            Ok(PreparedEvaluation::TopLevelDo(forms))
        } else {
            // Compiler.eval uses C.EVAL for collection heads whose symbol name
            // starts with def, including ordinary user functions with that prefix.
            // Other collections get a compiled wrapper function constant pool.
            let direct = match &form.kind {
                Kind::List(items) | Kind::Vector(items) | Kind::Set(items) =>
                    matches!(items.first().map(|item| &item.kind), Some(Kind::Symbol(name)) if name.name.starts_with("def")),
                Kind::Map(_) => false,
                // A transformer may return a live function or Var directly.
                // Non-collection Compiler.eval inputs use C.EVAL, too.
                _ => true,
            };
            let origin=origin.with_native_direct_constants(direct);
            CompilationSnapshot::new(environment, snapshot.phase(), snapshot.source_paths.clone(), snapshot.provided.clone())
                .prepare_with_origin(vec![form], span, expander, Some(&origin))
                .map(PreparedEvaluation::Fragment)
        }

}

impl MacroHost<'_> {
    /// Prepare native source against the persistent caller catalog. Execution
    /// stays in the ordinary owned Session entry, with its own runtime budget.
    pub(crate) fn prepare_native_input<T>(
        &mut self,
        prepare: impl FnOnce(&CompilationSnapshot, &mut dyn ExpansionHost) -> Result<T, SessionError>,
    ) -> Result<T, SessionError> {
        let _operation = self.fallback.as_ref().map(MacroService::operation);
        self.begin_declarations().map_err(SessionError::Compile)?;
        let outcome = (|| {
        let name = self.session.current_namespace().to_owned();
        let name = self.bridge.quote(&mut self.session, Form { span: 0..0, metadata: vec![], kind: Kind::Symbol(suss_reader::Symbol::new(name)) })?;
        let namespace = self.session.invoke_compiler_root(&self.namespace_interner, &[&name])?;
        let checkpoint = self.session.enter_compilation_namespace(&namespace)?;
        let snapshot = self.session.compilation_snapshot();
        let prepared = prepare(&snapshot, self);
        let outcome = self.native_preparation_result(prepared);
        let restoration = self.session.leave_compilation_namespace(&checkpoint);
        let outcome = match outcome { Err(error) => Err(error), Ok(value) => restoration.map(|()| value) };
        outcome
        })();
        if outcome.is_err() { self.end_declarations(false).map_err(SessionError::Compile)?; }
        outcome
    }
    pub(crate) fn finish_native_source(&mut self, linked: bool) -> Result<(), SessionError> {
        self.end_declarations(linked).map_err(SessionError::Compile)
    }
    pub(super) fn prepare_native_evaluation(
        &mut self, snapshot: &CompilationSnapshot, form: Form,
    ) -> Result<PreparedEvaluation, SessionError> {
        let _operation = self.fallback.as_ref().map(MacroService::operation);
        let result = prepare_with_expander(snapshot, form, self);
        self.native_preparation_result(result)
    }

    pub(crate) fn evaluate_native_form(
        &mut self,
        form: Form,
    ) -> Result<SessionValue, SessionError> {
        let mut pending = vec![form];
        let mut result = None;
        while let Some(form) = pending.pop() {
            let namespace = self.session.evaluation_namespace()?;
            let mut snapshot = self.session.compilation_snapshot();
            let phase = self.session.phase();
            let original = snapshot.environment.current_namespace(phase).to_owned();
            snapshot
                .environment
                .enter_namespace(phase, &namespace)
                .map_err(SessionError::Compile)?;
            snapshot.environment.replace_evaluation_aliases(phase, self.session.evaluation_aliases()?);
            self.session.synchronize_evaluation_macros(&mut snapshot)?;
            snapshot.environment.native_evaluation_namespace = true;
            let publish_namespace = self.session.evaluation_updates_input_namespace()?;
            match self.prepare_native_evaluation(&snapshot, form)? {
                PreparedEvaluation::MacroDeclaration(form) => {
                    self.enter_namespace(&namespace)?;
                    let outcome = (|| {
                        let origin = crate::portable::SourceOrigin::native_evaluation(self.session.native_owner());
                        let _operation = self.fallback.as_ref().map(MacroService::operation);
                        let prepared = self.prepare_macro_definition(&form, form.span.clone(), Some(&origin));
                        let mut pending = self.native_preparation_result(prepared)?;
                        pending.prepared.fragment.environment.enter_namespace(phase, &original).map_err(SessionError::Compile)?;
                        self.commit_macro_definition(pending).map(|(_function, variable)| variable)
                    })();
                    self.enter_namespace(&original)?;
                    result = Some(outcome?);
                }
                PreparedEvaluation::TopLevelDo(forms) => {
                    self.session.charge_host_work(forms.len() as u64 + 1)?;
                    pending.extend(forms.into_iter().rev());
                }
                PreparedEvaluation::Fragment(mut prepared) => {
                    prepared
                        .fragment
                        .environment
                        .enter_namespace(phase, &original)
                        .map_err(SessionError::Compile)?;
                    let outcome = self.session.eval_prepared(prepared);
                    if publish_namespace {
                        match self
                            .session
                            .evaluation_namespace()
                            .and_then(|namespace| self.session.enter_namespace(&namespace))
                        {
                            Ok(()) => {}
                            Err(error) if outcome.is_ok() => return Err(error),
                            Err(_) => {}
                        }
                    }
                    result = Some(outcome?);
                }
            }
        }
        result.ok_or_else(|| wasmtime::Error::msg("No compiled eval unit produced a value").into())
    }
}
