//! Replacement source -> HIR -> verified explicit IR -> shared-ABI fragments.
//! Native sessions use this pipeline; AOT and source macro integration remain incomplete.
//! Supports scalars, resolved cells, fixed closures/universal calls and numeric bootstrap.
mod emit;
mod function_names;
pub(crate) mod metered_map;
pub mod hir;
pub mod ir;
pub mod modules;
pub mod resolve;
mod source;
mod stdlib;
mod origin;
pub mod syntax_quote;
pub mod bootstrap;
pub mod artifact_cache;
pub mod artifact_identity;
pub mod core_bindings;
pub mod compiler_facts;
mod namespace_revision;
pub mod persistent;
pub use namespace_revision::NamespaceRevision;
pub mod compiler_names;
pub mod aot;
pub mod command;
pub use origin::{SourceOrigin, SourcePosition};
pub(crate) use origin::clone_form_metered;
use std::ops::Range;
use suss_reader::forms::{read_forms, resolve_conditionals};

#[derive(Debug, thiserror::Error)]
#[error("{message} at bytes {span:?}")]
pub struct Diagnostic {
    pub span: Range<usize>,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnalysisContext {
    Statement,
    Expression,
    Return,
}
impl AnalysisContext {
    /// Body contexts produce a return expression without granting recur scope.
    pub(crate) fn returning(self) -> Self {
        if self == Self::Expression { Self::Return } else { self }
    }
}

/// Expansion executes in a host's isolated compiled session, not the compiler.
pub struct ExpansionContext<'a> {
    pub environment: &'a resolve::Environment,
    /// Immutable namespace at entry to the enclosing top-level source form.
    /// Resolution uses the live environment above, including provisional defs.
    pub namespace_snapshot: &'a std::sync::Arc<hir::SourceNamespace>,
    /// Analyzer-minted, lazy live revision factory. Manually assembled contexts
    /// use None and retain the legacy fresh-capture behavior.
    pub namespace_revision: Option<NamespaceRevision<'a>>,
    pub origin: Option<&'a SourceOrigin>,
    pub phase: resolve::Phase,
    pub context: AnalysisContext,
    pub locals: &'a hir::SourceLocals,
    pub fields: &'a hir::SourceFields,
    pub function_scopes: &'a [std::sync::Arc<hir::FunctionScope>],
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MacroReload {
    Once,
    Reload,
    ReloadAll,
}
pub const DEFAULT_OPERATION_FUEL: u64 = 10_000_000;
pub trait ExpansionHost {
    /// Cumulative native work allowance for one preparation request. Compiled
    /// hosts return their configured Session allowance and debit guest/host work
    /// from that Session as well. The compiler enforces this fallback even when
    /// a custom host leaves its accounting hooks at their defaults.
    fn expansion_work_fuel(&self) -> u64 { DEFAULT_OPERATION_FUEL }
    /// One preparation request shares its expansion budget across transformer
    /// calls, recursive re-expansion and nested macro dependency preparation.
    fn begin_expansion_request(&mut self, _span: Range<usize>) -> Result<(), Diagnostic> {
        Ok(())
    }
    fn end_expansion_request(&mut self) {}

    /// Charge native analysis performed within the current expansion request.
    /// Hosts executing compiled transformers share this with their guest fuel.
    fn charge_expansion_work(&mut self, _work: u64, _span: Range<usize>) -> Result<(), Diagnostic> {
        Ok(())
    }

    /// Immutable selected macro source identities; None means unverified/unknown.
    fn artifact_dependencies(&self) -> Option<Vec<(String, String)>> { None }

    /// Analysis and macro execution have already happened. Hosts may reuse only
    /// emission bytes; staged catalogs and cells always come from this analysis.
    fn emit_fragment(
        &mut self, function: &ir::Function, phase: resolve::Phase,
        forms: &[suss_reader::forms::Form], origin: Option<&SourceOrigin>,
    ) -> Result<Vec<u8>, Diagnostic> {
        let _ = (phase, forms, origin);
        let mut remaining = self.expansion_work_fuel();
        compile_ir_metered(function, &mut |work| {
            self.charge_expansion_work(work, function.span.clone())?;
            remaining = remaining.checked_sub(work).ok_or_else(|| Diagnostic {
                span: function.span.clone(), message: "IR verification fuel exhausted".into(),
            })?;
            Ok(())
        })
    }

    fn supports_macro_imports(&self) -> bool {
        false
    }
    fn source_paths(&mut self, _paths: &[std::path::PathBuf]) {}
    fn load_macro_namespace(
        &mut self,
        _namespace: &str,
        span: Range<usize>,
    ) -> Result<Vec<String>, Diagnostic> {
        Err(Diagnostic { span, message: "Source macro imports require an isolated compiled macro session, not yet integrated".into() })
    }

    fn load_macro_namespace_with_policy(
        &mut self,
        namespace: &str,
        policy: MacroReload,
        span: Range<usize>,
    ) -> Result<Vec<String>, Diagnostic> {
        if policy == MacroReload::Once {
            self.load_macro_namespace(namespace, span)
        } else {
            Err(Diagnostic {
                span,
                message: "Source macro reload requires a compiled phase loading host".into(),
            })
        }
    }
    fn expand(
        &mut self,
        form: &suss_reader::forms::Form,
        context: ExpansionContext<'_>,
    ) -> Result<Option<suss_reader::forms::Form>, Diagnostic>;
}
pub(crate) struct NoExpansion;
impl ExpansionHost for NoExpansion {
    fn artifact_dependencies(&self) -> Option<Vec<(String, String)>> { Some(vec![]) }

    fn expand(
        &mut self,
        _: &suss_reader::forms::Form,
        _: ExpansionContext<'_>,
    ) -> Result<Option<suss_reader::forms::Form>, Diagnostic> {
        Ok(None)
    }
}
pub fn analyze(source: &str) -> Result<hir::Hir, Diagnostic> {
    analyze_in(
        source,
        &resolve::Environment::default(),
        resolve::Phase::Runtime,
    )
}

pub fn analyze_in(
    source: &str,
    environment: &resolve::Environment,
    phase: resolve::Phase,
) -> Result<hir::Hir, Diagnostic> {
    let forms = read_forms(source)
        .and_then(resolve_conditionals)
        .map_err(|error| Diagnostic {
            span: error.span,
            message: error.message,
        })?;
    Ok(analyze_selected_fragment(&forms, 0..source.len(), environment, phase,
        &mut NoExpansion, Some(&SourceOrigin::new(source, None)))?.hir)
}
pub fn compile(source: &str) -> Result<Vec<u8>, Diagnostic> {
    compile_in(
        source,
        &resolve::Environment::default(),
        resolve::Phase::Runtime,
    )
}

pub fn compile_in(
    source: &str,
    environment: &resolve::Environment,
    phase: resolve::Phase,
) -> Result<Vec<u8>, Diagnostic> {
    let hir = analyze_in(source, environment, phase)?;
    let ir = ir::lower(&hir)?;
    let wasm = compile_ir(&ir)?;
    artifact_identity::annotate_source(&wasm, phase, Some(&SourceOrigin::new(source, None)), Some(&[]))
        .map_err(|message| Diagnostic { span: 0..source.len(), message })
}

/// Emit a normalized function only after graph/type verification and Wasm validation.
pub fn compile_ir(function: &ir::Function) -> Result<Vec<u8>, Diagnostic> {
    let wasm = emit::emit(function)?;
    artifact_identity::annotate_ir(&wasm).map_err(|message| Diagnostic { span: function.span.clone(), message })
}

pub(crate) fn compile_ir_metered(
    function: &ir::Function,
    meter: &mut dyn FnMut(u64) -> Result<(), Diagnostic>,
) -> Result<Vec<u8>, Diagnostic> {
    ir::verify_metered(function, meter)?;
    compile_verified_ir(function, meter)
}
pub(crate) fn compile_verified_ir(
    function: &ir::Function, meter: &mut dyn FnMut(u64) -> Result<(), Diagnostic>,
) -> Result<Vec<u8>, Diagnostic> {
    let wasm = emit::emit_verified_metered(function, meter)?;
    artifact_identity::annotate_ir(&wasm).map_err(|message| Diagnostic { span: function.span.clone(), message })
}

/// A validated fragment and staged compiler state. Install cells in one shared
/// runtime before execution; reuse existing cells by Global identity. A compiler
/// error never changes the supplied Environment. This does not load source files.
#[derive(Clone)]
pub struct PreparedFragment {
    pub wasm: Vec<u8>,
    pub environment: resolve::Environment,
    pub cells: Vec<resolve::Global>,
    pub namespace_directive: Option<suss_reader::forms::Form>,
}
pub fn prepare_fragment(
    source: &str,
    environment: &resolve::Environment,
    phase: resolve::Phase,
) -> Result<PreparedFragment, Diagnostic> {
    let forms = read_forms(source).map_err(|error| Diagnostic {
        span: error.span,
        message: error.message,
    })?;
    prepare_fragment_forms_with_origin(forms, 0..source.len(), environment, phase, &mut NoExpansion, Some(&SourceOrigin::new(source, None)))
}
/// Compile owned reader/expanded forms without printing or rereading them.
/// Conditional selection and namespace preparation preserve original form spans
/// and metadata. The caller supplies the enclosing source/call-site span.
pub fn prepare_fragment_forms(
    forms: Vec<suss_reader::forms::Form>,
    span: Range<usize>,
    environment: &resolve::Environment,
    phase: resolve::Phase,
) -> Result<PreparedFragment, Diagnostic> {
    let forms = resolve_conditionals(forms).map_err(|error| Diagnostic {
        span: error.span,
        message: error.message,
    })?;
    prepare_selected_fragment(forms, span, environment, phase)
}
pub(crate) fn prepare_selected_fragment(
    forms: Vec<suss_reader::forms::Form>,
    span: Range<usize>,
    environment: &resolve::Environment,
    phase: resolve::Phase,
) -> Result<PreparedFragment, Diagnostic> {
    prepare_selected_fragment_with_expander(forms, span, environment, phase, &mut NoExpansion)
}
pub fn prepare_fragment_forms_with_expander(
    forms: Vec<suss_reader::forms::Form>,
    span: Range<usize>,
    environment: &resolve::Environment,
    phase: resolve::Phase,
    expander: &mut dyn ExpansionHost,
) -> Result<PreparedFragment, Diagnostic> {
    prepare_fragment_forms_with_origin(forms, span, environment, phase, expander, None)
}
/// Compile forms with the exact source they refer to; no printing or rereading.
/// None explicitly means no origin is available for these forms.
pub fn prepare_fragment_forms_with_origin(
    forms: Vec<suss_reader::forms::Form>, span: Range<usize>,
    environment: &resolve::Environment, phase: resolve::Phase,
    expander: &mut dyn ExpansionHost, origin: Option<&SourceOrigin>,
) -> Result<PreparedFragment, Diagnostic> {
    let mut request = ExpansionRequest::new(expander, span.clone())?;
    let forms = suss_reader::forms::resolve_conditionals_metered(forms, &mut |work| {
        request.charge_expansion_work(work, span.clone()).map_err(|error| suss_reader::ParseError {
            message: error.message, span: error.span, expected: vec![],
        })
    }).map_err(|error| Diagnostic { span: error.span, message: error.message })?;
    prepare_selected_fragment_with_origin(forms, span, environment, phase, &mut request, origin)
}
pub(crate) fn prepare_selected_fragment_with_expander(
    forms: Vec<suss_reader::forms::Form>, span: Range<usize>,
    environment: &resolve::Environment, phase: resolve::Phase,
    expander: &mut dyn ExpansionHost,
) -> Result<PreparedFragment, Diagnostic> {
    prepare_selected_fragment_with_origin(forms, span, environment, phase, expander, None)
}
pub(crate) fn prepare_selected_fragment_with_origin(
    forms: Vec<suss_reader::forms::Form>, span: Range<usize>,
    environment: &resolve::Environment, phase: resolve::Phase,
    expander: &mut dyn ExpansionHost, origin: Option<&SourceOrigin>,
) -> Result<PreparedFragment, Diagnostic> {
    let mut request = ExpansionRequest::new(expander, span.clone())?;
    request.charge_expansion_work(
        u64::try_from(forms.len()).unwrap_or(u64::MAX),
        span.clone(),
    )?;
    // Analysis owns its HIR/source facts; retain the actual selected syntax
    // for both analysis and emission instead of cloning the complete input.
    let analyzed = analyze_selected_fragment(
        &forms,
        span.clone(),
        environment,
        phase,
        &mut request,
        origin,
    )?;
    let function = ir::lower_metered(&analyzed.hir, &mut |work| {
        request.charge_expansion_work(work, analyzed.hir.span.clone())
    })?;
    let wasm = request.emit_fragment(&function, phase, &forms, origin)?;
    let dependencies = request.artifact_dependencies();
    let wasm = artifact_identity::annotate_source(&wasm, phase, origin, dependencies.as_deref())
        .map_err(|message| Diagnostic { span: function.span.clone(), message })?;
    Ok(PreparedFragment { wasm, environment: analyzed.environment,
        cells: analyzed.cells, namespace_directive: analyzed.namespace_directive })
}

pub(crate) struct AnalyzedFragment {
    pub hir: hir::Hir,
    pub environment: resolve::Environment,
    pub cells: Vec<resolve::Global>,
    pub namespace_directive: Option<suss_reader::forms::Form>,
}
pub(crate) struct ExpansionRequest<'a> {
    host: &'a mut dyn ExpansionHost,
    remaining: u64,
}
impl<'a> ExpansionRequest<'a> {
    pub(crate) fn new(host: &'a mut dyn ExpansionHost, span: Range<usize>) -> Result<Self, Diagnostic> {
        host.begin_expansion_request(span)?;
        let remaining = host.expansion_work_fuel();
        hir::cow_trace_reset(remaining);
        Ok(Self { host, remaining })
    }
}
impl ExpansionHost for ExpansionRequest<'_> {
    fn expansion_work_fuel(&self) -> u64 { self.remaining }
    fn charge_expansion_work(&mut self, work: u64, span: Range<usize>) -> Result<(), Diagnostic> {
        let result = (|| {
            self.host.charge_expansion_work(work, span.clone())?;
            self.remaining = self.remaining.checked_sub(work).ok_or_else(|| Diagnostic {
                span,
                message: "Macro expansion native work fuel exhausted".into(),
            })?;
            Ok(())
        })();
        hir::cow_trace_charge(work, &result);
        result
    }

    fn emit_fragment(
        &mut self, function: &ir::Function, phase: resolve::Phase,
        forms: &[suss_reader::forms::Form], origin: Option<&SourceOrigin>,
    ) -> Result<Vec<u8>, Diagnostic> {
        self.host.emit_fragment(function, phase, forms, origin)
    }
    fn artifact_dependencies(&self) -> Option<Vec<(String, String)>> {
        self.host.artifact_dependencies()
    }
    fn supports_macro_imports(&self) -> bool { self.host.supports_macro_imports() }
    fn source_paths(&mut self, paths: &[std::path::PathBuf]) { self.host.source_paths(paths) }
    fn load_macro_namespace(&mut self, namespace: &str, span: Range<usize>) -> Result<Vec<String>, Diagnostic> {
        self.host.load_macro_namespace(namespace, span)
    }
    fn load_macro_namespace_with_policy(&mut self, namespace: &str, policy: MacroReload, span: Range<usize>) -> Result<Vec<String>, Diagnostic> {
        self.host.load_macro_namespace_with_policy(namespace, policy, span)
    }
    fn expand(&mut self, form: &suss_reader::forms::Form, context: ExpansionContext<'_>) -> Result<Option<suss_reader::forms::Form>, Diagnostic> {
        self.host.expand(form, context)
    }
}
impl Drop for ExpansionRequest<'_> {
    fn drop(&mut self) {
        self.host.end_expansion_request();
    }
}
pub(crate) fn analyze_selected_fragment(
    forms: &[suss_reader::forms::Form], span: Range<usize>,
    environment: &resolve::Environment, phase: resolve::Phase,
    expander: &mut dyn ExpansionHost, origin: Option<&SourceOrigin>,
) -> Result<AnalyzedFragment, Diagnostic> {
    let mut request = ExpansionRequest::new(expander, span.clone())?;
    let mut snapshot = environment.clone();
    let namespace_directive = source::namespace(forms, &mut snapshot, phase, &mut request)?;
    // The old owned analysis input kept the consumed ns node's metadata and
    // span while replacing only its kind. Stage exactly that one node, keeping
    // its position and analysis context; emission still sees the original ns.
    let analysis_head = namespace_directive.as_ref().map(|directive| {
        let bytes = directive.metadata.len().checked_mul(std::mem::size_of::<suss_reader::forms::Form>())
            .and_then(|bytes| u64::try_from(bytes).ok())
            .ok_or_else(|| Diagnostic { span: directive.span.clone(), message: "Namespace metadata work overflow".into() })?;
        request.charge_expansion_work(bytes, directive.span.clone())?;
        let mut metadata = Vec::new();
        metadata.try_reserve_exact(directive.metadata.len()).map_err(|_| Diagnostic {
            span: directive.span.clone(), message: "Unable to reserve namespace analysis metadata".into(),
        })?;
        for item in &directive.metadata {
            metadata.push(clone_form_metered(item, &mut |work| request.charge_expansion_work(work, item.span.clone()))?);
        }
        Ok::<_, Diagnostic>(suss_reader::forms::Form {
            span: directive.span.clone(), metadata, kind: suss_reader::forms::Kind::Nil,
        })
    }).transpose()?;
    let (hir, environment) = hir::prepare_with_origin_and_first_override(
        forms, span, &snapshot, phase, &mut request, origin, analysis_head.as_ref(),
    )?;
    let cells = environment
        .cells()
        .into_iter()
        .filter(|cell| cell.phase() == phase)
        .collect();
    Ok(AnalyzedFragment {
        hir, environment, cells, namespace_directive,
    })
}


#[cfg(test)]
mod borrowed_analysis_tests {
    use super::*;
    use suss_reader::forms::{Form, Kind};

    struct ObserveInput {
        expected: Vec<Form>,
        // Test-only address observations; never dereferenced or used as authority.
        addresses: Vec<usize>,
        seen: Vec<usize>,
        emitted: bool,
        fail_on_second: bool,
        limit: u64,
        spent: u64,
    }
    impl ObserveInput {
        fn new(forms: &[Form]) -> Self {
            Self { expected: forms.to_vec(), addresses: forms.iter().map(|form| form as *const Form as usize).collect(),
                seen: vec![], emitted: false, fail_on_second: false, limit: DEFAULT_OPERATION_FUEL, spent: 0 }
        }
    }
    impl ExpansionHost for ObserveInput {
        fn expansion_work_fuel(&self) -> u64 { self.limit.saturating_sub(self.spent) }
        fn charge_expansion_work(&mut self, work: u64, span: Range<usize>) -> Result<(), Diagnostic> {
            let next = self.spent.checked_add(work).filter(|next| *next <= self.limit)
                .ok_or_else(|| Diagnostic { span, message: "borrowed-input cumulative budget exhausted".into() })?;
            self.spent = next;
            Ok(())
        }
        fn expand(&mut self, form: &Form, _: ExpansionContext<'_>) -> Result<Option<Form>, Diagnostic> {
            if let Some(index) = self.expected.iter().position(|expected| expected.span == form.span) {
                assert_eq!(form as *const Form as usize, self.addresses[index], "analysis must borrow the actual selected input");
                assert_eq!(form, &self.expected[index]);
                self.seen.push(index);
                if self.fail_on_second && index == 2 {
                    return Err(Diagnostic { span: form.span.clone(), message: "injected borrowed-input expansion failure".into() });
                }
            }
            Ok(None)
        }
        fn emit_fragment(&mut self, _: &ir::Function, _: resolve::Phase, forms: &[Form], _: Option<&SourceOrigin>) -> Result<Vec<u8>, Diagnostic> {
            assert_eq!(forms, self.expected);
            assert_eq!(format!("{forms:?}"), format!("{:?}", self.expected));
            assert_eq!(forms.iter().map(|form| form as *const Form as usize).collect::<Vec<_>>(), self.addresses);
            self.emitted = true;
            // Stop at the inspected emission boundary; this focused test does
            // not claim artifact execution or alter the real emitter's work.
            Err(Diagnostic { span: 0..0, message: "inspected original emission input".into() })
        }
    }

    #[test]
    fn preparation_borrows_actual_input_and_preserves_emission_header_order_error_and_retry() {
        let source = "(ns borrowed.input) (def first 1) (def second first)";
        for phase in [resolve::Phase::Runtime, resolve::Phase::Macro] {
            let environment = resolve::Environment::default();
            let old_namespace = environment.current_namespace(phase).to_owned();
            // Repeat after both expansion and emission errors; no source
            // rewriting or environment publication escapes either failure.
            for fail_on_second in [true, false, false] {
                let forms = read_forms(source).unwrap();
                let mut host = ObserveInput::new(&forms);
                host.fail_on_second = fail_on_second;
                let error = prepare_selected_fragment_with_origin(forms, 0..source.len(), &environment, phase, &mut host, None).err().expect("expected injected preparation error");
                assert_eq!(host.seen, vec![1, 2]);
                assert_eq!(host.emitted, !fail_on_second);
                assert_eq!(error.message, if fail_on_second { "injected borrowed-input expansion failure" } else { "inspected original emission input" });
                assert_eq!(environment.current_namespace(phase), old_namespace);
                assert!(matches!(host.expected[0].kind, Kind::List(_)));
            }
        }
    }

    #[test]
    fn borrowed_analysis_preserves_consumed_namespace_frame_and_owned_source_after_inputs_drop() {
        let source = "^:kept (ns borrowed.frames) (def first 1) first";
        for phase in [resolve::Phase::Runtime, resolve::Phase::Macro] {
            let forms = read_forms(source).unwrap();
            let original = forms.clone();
            let environment = resolve::Environment::default();
            let analyzed = analyze_selected_fragment(&forms, 0..source.len(), &environment, phase, &mut NoExpansion, None).unwrap();
            assert_eq!(forms, original);
            assert_eq!(analyzed.namespace_directive.as_ref().unwrap(), &forms[0]);
            let hir::Expression::Do(items) = &analyzed.hir.kind else { panic!("top-level body") };
            assert_eq!(items.len(), 3);
            assert!(matches!(items[0].kind, hir::Expression::Literal(hir::Literal::Nil)));
            assert_eq!(items[0].span, forms[0].span);
            assert_eq!(items[0].metadata, forms[0].metadata);
            let source_head = items[0].source.as_ref().expect("consumed ns source frame");
            assert!(matches!(source_head.form.kind, Kind::Nil));
            assert_eq!(source_head.form.span, forms[0].span);
            assert_eq!(source_head.form.metadata, forms[0].metadata);
            assert_eq!(source_head.context, AnalysisContext::Statement);
            assert_eq!(items[2].source.as_ref().unwrap().context, AnalysisContext::Statement);
            drop(forms);
            assert_eq!(analyzed.environment.current_namespace(phase), "borrowed.frames");
            assert_eq!(analyzed.namespace_directive.as_ref().unwrap(), &original[0]);
            assert_eq!(items[1].source.as_ref().unwrap().form, original[1]);
        }
    }

    #[test]
    fn borrowed_preparation_keeps_cumulative_budget_across_analysis_and_lowering() {
        let source = "(ns borrowed.budget) (def first 1) (def second first)";
        for phase in [resolve::Phase::Runtime, resolve::Phase::Macro] {
            let environment = resolve::Environment::default();
            let forms = read_forms(source).unwrap();
            let mut baseline = ObserveInput::new(&forms);
            let error = prepare_selected_fragment_with_origin(forms, 0..source.len(), &environment, phase, &mut baseline, None).err().expect("expected injected preparation error");
            assert_eq!(error.message, "inspected original emission input");
            assert!(baseline.spent > 1 && baseline.emitted);
            let forms = read_forms(source).unwrap();
            let original = forms.clone();
            let mut bounded = ObserveInput::new(&forms);
            bounded.limit = baseline.spent - 1;
            let error = prepare_selected_fragment_with_origin(forms, 0..source.len(), &environment, phase, &mut bounded, None).err().expect("expected injected preparation error");
            assert_eq!(error.message, "borrowed-input cumulative budget exhausted");
            assert!(!bounded.emitted);
            assert_eq!(bounded.expected, original);
            // Retry with the original allowance and independently read input.
            let forms = read_forms(source).unwrap();
            let mut retry = ObserveInput::new(&forms);
            let error = prepare_selected_fragment_with_origin(forms, 0..source.len(), &environment, phase, &mut retry, None).err().expect("expected injected preparation error");
            assert_eq!(error.message, "inspected original emission input");
            assert_eq!(retry.spent, baseline.spent);
        }
    }
}
