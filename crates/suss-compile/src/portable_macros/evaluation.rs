//! Clojure Compiler.eval's top-level compilation boundary. Each executable
//! unit still goes through the ordinary analyzer/emitter; this does not walk
//! expression bodies or evaluate language operators on the host.
use super::*;
use crate::portable_session::CompilationSnapshot;

pub(crate) enum PreparedEvaluation {
    Fragment(crate::portable::modules::PreparedInput),
    TopLevelDo(Vec<Form>),
}

impl MacroHost<'_> {
    pub(super) fn prepare_native_evaluation(
        &mut self,
        snapshot: &CompilationSnapshot,
        mut form: Form,
    ) -> Result<PreparedEvaluation, SessionError> {
        let span = form.span.clone();
        let origin = crate::portable::SourceOrigin::new("", None);
        {
            let mut request = crate::portable::ExpansionRequest::new(self, span.clone())
                .map_err(SessionError::Compile)?;
            request
                .charge_expansion_work(
                    snapshot
                        .environment
                        .current_namespace(snapshot.phase())
                        .len() as u64
                        + 1,
                    span.clone(),
                )
                .map_err(SessionError::Compile)?;
            let namespace = Arc::new(crate::portable::hir::SourceNamespace::capture(
                &snapshot.environment,
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
                    environment: &snapshot.environment,
                    namespace_snapshot: &namespace,
                    origin: Some(&origin),
                    phase: snapshot.phase(),
                    context: crate::portable::AnalysisContext::Expression,
                    locals: &locals,
                    fields: &fields,
                    function_scopes: &[],
                };
                match request
                    .expand(&form, context)
                    .map_err(SessionError::Compile)?
                {
                    Some(expanded) => form = expanded,
                    None => break,
                }
            }
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
            snapshot
                .prepare_with_origin(vec![form], span, self, Some(&origin))
                .map(PreparedEvaluation::Fragment)
        }
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
            snapshot.environment.native_evaluation_namespace = true;
            let publish_namespace = self.session.evaluation_updates_input_namespace()?;
            match self.prepare_native_evaluation(&snapshot, form)? {
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
