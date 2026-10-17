//! Original compiler binding facts for compiled macro environments.
//! Retain actual declarations and analyzed initializers; never reconstruct facts
//! by evaluating source or replacing unknown information with a scalar stand-in.
use super::*;
use std::sync::Arc;

/// Owned lexical revisions. Entries retain their own immutable fact revisions.
/// The guest locals projection depends on the pair of locals and fields owners;
/// neither declaration identity nor equal map contents identifies a revision.
pub type SourceLocals = Arc<HashMap<String, Arc<LocalBinding>>>;
pub type SourceFields = Arc<HashMap<String, Arc<FieldBinding>>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalKind {
    Let,
    Loop,
    Argument { index: usize, rest: bool },
    FunctionName,
    Catch,
}

/// Source adaptation facts, separate from physical parameter/loop lowering.
#[derive(Debug, Clone)]
pub enum SourceRole {
    Plain,
    FunctionName {
        variadic: bool,
        /// Absent on the initial self binding; present only after parameter staging.
        methods: Option<Arc<SourceFunctionParameters>>,
    },
    PrivateCatch {
        anchor: std::ops::Range<usize>,
    },
    CatchBinding {
        hidden: Arc<LocalBinding>,
        access: Arc<Hir>,
    },
    MethodArgument {
        index: usize,
        rest: bool,
    },
    MethodThis {
        type_declaration: Arc<Form>,
        namespace: String,
        protocol_receiver: bool,
        /// Actual source receiver form, independent of a same-named last formal.
        receiver_declaration: Arc<Form>,
        /// Visible source argument shadow; Object receivers may shadow a user arg.
        argument: Option<Arc<LocalBinding>>,
        /// Actual lowered receiver access; inspecting it does not read a value.
        access: Arc<Hir>,
    },
}

/// Immutable syntax owner. Definition reuse retains only the provisional
/// record captured before analysis, never the completed initializer graph.
#[derive(Debug, Clone)]
pub struct FunctionForm {
    owner: FunctionFormOwner,
}
#[derive(Debug, Clone)]
enum FunctionFormOwner {
    DefinitionInitializer(Arc<super::super::resolve::DefinitionInfo>),
    Owned(Arc<Form>),
}
impl FunctionForm {
    pub fn as_form(&self) -> &Form {
        match &self.owner {
            FunctionFormOwner::DefinitionInitializer(owner) => owner.initializer_form.as_ref().expect("captured definition initializer"),
            FunctionFormOwner::Owned(form) => form,
        }
    }

    fn capture(form: &Form, initializer: Option<&DefinitionInitializer<'_>>, meter: &mut CowMeter<'_>) -> Result<Self, Diagnostic> {
        if let Some(initializer) = initializer.filter(|candidate| std::ptr::eq(candidate.input, form)
            && candidate.owner.initializer_form.is_some()
            && !candidate.owner.analysis_completed && candidate.owner.initializer.is_none()) {
            let _trace = CowTraceScope::new("function_form_reuse");
            meter(1)?; // actual owner reference retention, no syntax allocation
            return Ok(Self { owner: FunctionFormOwner::DefinitionInitializer(initializer.owner.clone()) });
        }
        let form = super::super::clone_form_metered(form, meter)?;
        cow_charge(meter, 1, std::mem::size_of::<Form>())?;
        cow_charge(meter, 2, std::mem::size_of::<usize>())?;
        Ok(Self { owner: FunctionFormOwner::Owned(Arc::new(form)) })
    }
}

/// Named source function scope; a definition hint does not create a lexical ID.
#[derive(Debug, Clone)]
pub struct FunctionScope {
    /// Actual function syntax before any method body expansion or lowering.
    pub function_form: FunctionForm,
    pub declaration: Form,
    pub origin: Option<Arc<super::super::SourceOrigin>>,
    pub namespace: String,
    pub declaration_context: super::super::AnalysisContext,
    /// Actual declaration environment, before the new self name is installed.
    pub scope: Arc<SourceNamespace>,
    pub namespace_snapshot: Arc<SourceNamespace>,
    pub locals: SourceLocals,
    pub fields: SourceFields,
    pub phase: Phase,
    pub parents: Arc<[Arc<FunctionScope>]>,
    pub self_binding: Option<Arc<LocalBinding>>,
    pub shadow: Option<Arc<LocalBinding>>,
    pub shadow_field: Option<Arc<FieldBinding>>,
}

#[derive(Debug, Clone)]
pub struct FieldBinding {
    /// Shared declaration identity across immutable snapshot clones.
    pub identity: Arc<()>,
    pub declaration: Form,
    pub origin: Option<Arc<super::super::SourceOrigin>>,
    pub index: usize,
    pub mutable: bool,
    /// Actual lowered access expression; inspecting it does not read the field.
    pub access: Hir,
}

#[derive(Debug, Clone)]
pub struct LocalBinding {
    /// Shared declaration identity; lowering remaps do not invent declarations.
    pub identity: Arc<()>,
    /// Current lowered binding used for lexical lookup, including loop remapping.
    pub id: BindingId,
    pub ty: Type,
    /// Reader or expansion declaration, preserving source/call-site span and metadata.
    pub declaration: Arc<Form>,
    pub origin: Option<Arc<super::super::SourceOrigin>>,
    pub kind: LocalKind,
    pub declaration_context: super::super::AnalysisContext,
    pub source_role: SourceRole,
    /// Actual analyzed initializer; absent for parameters/self/private payloads.
    pub initializer: Option<Arc<Hir>>,
    /// Previous lexical declaration, excluding compiler-only ID remapping.
    pub shadow: Option<Arc<LocalBinding>>,
    pub shadow_field: Option<Arc<FieldBinding>>,
}
impl LocalBinding {
    /// Portable source role without changing the actual lowered binding identity.
    pub fn source_kind(&self) -> LocalKind {
        match &self.source_role {
            SourceRole::Plain
            | SourceRole::FunctionName { .. }
            | SourceRole::PrivateCatch { .. } => self.kind,
            SourceRole::CatchBinding { .. } => LocalKind::Let,
            SourceRole::MethodArgument { index, rest } => LocalKind::Argument {
                index: *index,
                rest: *rest,
            },
            SourceRole::MethodThis { .. } => LocalKind::Let,
        }
    }
}
impl Analyzer<'_> {
    pub(super) fn record_method_roles(
        &mut self,
        parameters: &[Parameter],
        declarations: &[Form],
        type_declaration: &Form,
        object_method: bool,
        receiver_argument: Option<Arc<LocalBinding>>,
        receiver_declaration: Arc<Form>,
    ) -> Result<(), Diagnostic> {
        let _trace = CowTraceScope::new("method_roles");
        fn object_argument_shadow(
            mut shadow: Option<Arc<LocalBinding>>, parameters: &[Parameter], meter: &mut CowMeter<'_>,
        ) -> Result<Option<Arc<LocalBinding>>, Diagnostic> {
            let mut rebuilt = Vec::new();
            let tail = loop {
                meter(1)?;
                let Some(original) = shadow else { break None };
                if original.id == parameters[0].id {
                    shadow = original.shadow.clone();
                    continue;
                }
                let mut found = None;
                for (index, parameter) in parameters.iter().enumerate() {
                    meter(1)?;
                    if parameter.id == original.id { found = Some(index); break; }
                }
                let Some(index) = found else { break Some(original) };
                let mut binding = cow_local(&original, meter)?;
                binding.source_role = SourceRole::MethodArgument {
                    index: index - 1, rest: matches!(binding.kind, LocalKind::Argument { rest: true, .. }),
                };
                shadow = binding.shadow.take();
                // Charge each pending record before staging it; no guest calls.
                if rebuilt.len() == rebuilt.capacity() {
                    let growth = rebuilt.capacity().max(1);
                    let requested = rebuilt.capacity().checked_add(growth)
                        .ok_or_else(|| fail(0..0, "Source ownership capacity overflow"))?;
                    cow_charge(meter, requested, std::mem::size_of::<LocalBinding>())?;
                    cow_charge(meter, rebuilt.len(), std::mem::size_of::<LocalBinding>())?;
                    rebuilt.try_reserve_exact(growth).map_err(|_| fail(0..0, "Unable to reserve source shadow revisions"))?;
                }
                rebuilt.push(binding);
            };
            let mut tail = tail;
            for mut binding in rebuilt.into_iter().rev() {
                meter(1)?;
                binding.shadow = tail;
                tail = Some(Arc::new(binding));
            }
            Ok(tail)
        }
        // Last same-named user parameter wins, then source this-as binds the
        // receiver outside the body. Do not overwrite that let with an arg role.
        for (index, parameter) in parameters.iter().enumerate().skip(1) {
            let shadow = if object_method {
                let original = self.locals[&parameter.name].shadow.clone();
                Some(object_argument_shadow(original, parameters, &mut |work|
                    self.expander.charge_expansion_work(work, declarations[0].span.clone()))?)
            } else { None };
            let binding = self.local_revision_mut(&parameter.name)?;
            binding.source_role = SourceRole::MethodArgument {
                index: index - usize::from(object_method),
                rest: matches!(binding.kind, LocalKind::Argument { rest: true, .. }),
            };
            if object_method {
                binding.shadow = shadow.expect("object method shadow");
            }
        }
        let receiver = &parameters[0];
        let mut repeated = false;
        for parameter in &parameters[1..] {
            self.expander.charge_expansion_work(1, declarations[0].span.clone())?;
            cow_charge(&mut |work| self.expander.charge_expansion_work(work, declarations[0].span.clone()), parameter.name.len(), 1)?;
            if parameter.name == receiver.name { repeated = true; break; }
        }
        let argument = if repeated {
            let mut argument = cow_local(&receiver_argument.expect("visible receiver-name argument"),
                &mut |work| self.expander.charge_expansion_work(work, declarations[0].span.clone()))?;
            let LocalKind::Argument { index, rest } = argument.kind else {
                unreachable!()
            };
            argument.source_role = SourceRole::MethodArgument {
                index: index - usize::from(object_method),
                rest,
            };
            // Object's physical receiver is not a source formal in its fn.
            // Preserve any true outer shadow behind that implicit parameter.
            if object_method {
                argument.shadow = object_argument_shadow(argument.shadow.take(), parameters,
                    &mut |work| self.expander.charge_expansion_work(work, declarations[0].span.clone()))?;
            }
            Some(Arc::new(argument))
        } else if !object_method {
            let mut argument = cow_local(&self.locals[&receiver.name],
                &mut |work| self.expander.charge_expansion_work(work, declarations[0].span.clone()))?;
            argument.source_role = SourceRole::Plain;
            Some(Arc::new(argument))
        } else {
            None
        };
        let mut meter = |work| self.expander.charge_expansion_work(work, declarations[0].span.clone());
        let role = SourceRole::MethodThis {
            type_declaration: cow_shared_form(type_declaration, &mut meter)?,
            namespace: cow_text(self.environment.current_namespace(self.phase), &mut meter)?,
            protocol_receiver: !object_method,
            receiver_declaration,
            argument,
            access: {
                cow_charge(&mut meter, 1, std::mem::size_of::<Hir>())?;
                cow_charge(&mut meter, 2, std::mem::size_of::<usize>())?;
                Arc::new(self.local(&declarations[0], receiver.id))
            },
        };
        self.local_revision_mut(&receiver.name)?.source_role = role;
        Ok(())
    }

    pub(super) fn enter_function_scope(
        &mut self,
        function_form: &Form,
        declaration: Option<&Form>,
        self_binding: Option<Arc<LocalBinding>>,
        initializer: Option<&DefinitionInitializer<'_>>,
    ) -> Result<(), Diagnostic> {
        let _trace = CowTraceScope::new("function_capture");
        let Some(declaration) = declaration else {
            return Ok(());
        };
        let Kind::Symbol(name) = &declaration.kind else {
            unreachable!("function name");
        };
        cow_charge(&mut |work| self.expander.charge_expansion_work(work, function_form.span.clone()), name.name.len(), 2)?;
        let (shadow, shadow_field) = if let Some(binding) = &self_binding {
            (binding.shadow.clone(), binding.shadow_field.clone())
        } else {
            let shadow = self.locals.get(&name.name).cloned();
            let field = if shadow.is_none() {
                self.fields.get(&name.name).cloned()
            } else {
                None
            };
            (shadow, field)
        };
        let mut locals = self.locals.clone();
        let mut meter = |work| self.expander.charge_expansion_work(work, function_form.span.clone());
        if let Some(binding) = &self_binding {
            if let Some(shadow) = &binding.shadow {
                cow_insert(&mut locals, &name.name, shadow.clone(), &mut meter)?;
            } else {
                cow_remove(&mut locals, &name.name, &mut meter)?;
            }
        }
        let function_form_copy = {
            let _trace = CowTraceScope::new("function_form");
            FunctionForm::capture(function_form, initializer, &mut meter)?
        };
        let declaration_copy = {
            let _trace = CowTraceScope::new("function_name");
            super::super::clone_form_metered(declaration, &mut meter)?
        };
        let origin = cow_origin(&self.origin, &mut meter)?;
        let namespace = cow_text(self.environment.current_namespace(self.phase), &mut meter)?;
        let parents = cow_parent_scopes(&self.function_scopes, &mut meter)?;
        let mut grown_scopes = cow_scope_growth(&self.function_scopes, &mut meter)?;
        cow_charge(&mut meter, 1, std::mem::size_of::<FunctionScope>())?;
        cow_charge(&mut meter, 2, std::mem::size_of::<usize>())?;
        let scope = self.capture_source_namespace();
        let record = Arc::new(FunctionScope {
            function_form: function_form_copy,
            declaration: declaration_copy,
            origin,
            namespace,
            declaration_context: self
                .analysis_contexts
                .last()
                .copied()
                .expect("source analysis context"),
            scope,
            namespace_snapshot: self.namespace_snapshot.as_ref().expect("top-level source snapshot").clone(),
            locals,
            fields: self.fields.clone(),
            phase: self.phase,
            parents,
            self_binding,
            shadow,
            shadow_field,
        });
        // All fallible staging is complete. Neither push can allocate now.
        if let Some(mut staged) = grown_scopes.take() {
            staged.push(record);
            self.function_scopes = staged;
        } else {
            self.function_scopes.push(record);
        }
        Ok(())
    }

    pub(super) fn insert_local(
        &mut self, declaration: &Form, id: BindingId, ty: Type,
        kind: LocalKind, initializer: Option<Arc<Hir>>,
    ) -> Result<Option<Arc<LocalBinding>>, Diagnostic> {
        let _trace = CowTraceScope::new("insert_local");
        let Kind::Symbol(name) = &declaration.kind else { unreachable!("validated declaration") };
        let span = declaration.span.clone();
        let mut meter = |work| self.expander.charge_expansion_work(work, span.clone());
        cow_charge(&mut meter, name.name.len(), 2)?;
        let shadow = self.locals.get(&name.name).cloned();
        let shadow_field = if shadow.is_none() { self.fields.get(&name.name).cloned() } else { None };
        cow_charge(&mut meter, 1, std::mem::size_of::<LocalBinding>())?;
        let record = Arc::new(LocalBinding {
            identity: Arc::new(()), id, ty,
            declaration: cow_shared_form(declaration, &mut meter)?,
            origin: cow_origin(&self.origin, &mut meter)?, kind,
            declaration_context: self.analysis_contexts.last().copied().expect("source analysis context"),
            source_role: SourceRole::Plain, initializer, shadow, shadow_field,
        });
        cow_insert(&mut self.locals, &name.name, record, &mut meter)
    }
    /// Changing facts forks the map revision and the affected entry revision.
    pub(super) fn local_revision_mut(&mut self, name: &str) -> Result<&mut LocalBinding, Diagnostic> {
        // Charge the lookup needed to recover the declaration's diagnostic span.
        cow_charge(&mut |work| self.expander.charge_expansion_work(work, 0..0), name.len(), 1)?;
        let span = self.locals.get(name).expect("existing local").declaration.span.clone();
        let expander = &mut self.expander;
        local_revision_mut(&mut self.locals, name, &mut |work| expander.charge_expansion_work(work, span.clone()))
    }
    pub(super) fn remap_local(&mut self, name: &str, id: BindingId, ty: Type) -> Result<(), Diagnostic> {
        let record = self.local_revision_mut(name)?;
        record.id = id;
        record.ty = ty;
        Ok(())
    }
    pub(super) fn remove_local(&mut self, name: &str, span: Range<usize>) -> Result<(), Diagnostic> {
        let expander = &mut self.expander;
        cow_remove(&mut self.locals, name, &mut |work| expander.charge_expansion_work(work, span.clone()))?;
        Ok(())
    }
    pub(super) fn insert_field(&mut self, name: &str, field: Arc<FieldBinding>, span: Range<usize>) -> Result<(), Diagnostic> {
        let expander = &mut self.expander;
        cow_insert(&mut self.fields, name, field, &mut |work| expander.charge_expansion_work(work, span.clone()))?;
        Ok(())
    }
    pub(super) fn clone_origin_value(&mut self, span: Range<usize>) -> Result<Option<super::super::SourceOrigin>, Diagnostic> {
        let mut meter = |work| self.expander.charge_expansion_work(work, span.clone());
        meter(1)?;
        if let Some(origin) = &self.origin { origin_value_work(origin, &mut meter)?; }
        Ok(self.origin.as_deref().cloned())
    }
    pub(super) fn clone_local_form(&mut self, form: &Form) -> Result<Form, Diagnostic> {
        super::super::clone_form_metered(form, &mut |work| self.expander.charge_expansion_work(work, form.span.clone()))
    }
    pub(super) fn capture_local_access(&mut self, node: Hir) -> Result<Arc<Hir>, Diagnostic> {
        {
            let _trace = CowTraceScope::new("local_access_owner");
            cow_charge(&mut |work| self.expander.charge_expansion_work(work, node.span.clone()), 1, std::mem::size_of::<Hir>())?;
            cow_charge(&mut |work| self.expander.charge_expansion_work(work, node.span.clone()), 2, std::mem::size_of::<usize>())?;
        }
        cow_trace_expression("capture_arc", &node.kind);
        Ok(Arc::new(node))
    }
    pub(super) fn clone_local_hir(&mut self, node: &Hir) -> Result<Hir, Diagnostic> {
        cow_hir(node, &mut |work| self.expander.charge_expansion_work(work, node.span.clone()))
    }

}


// Opt-in native-work attribution only. No rates, budgets, or guest calls change.
#[derive(Default)]
struct CowTraceState {
    stack: Vec<&'static str>,
    totals: BTreeMap<String, (u64, u64, u64)>,
    events: BTreeMap<&'static str, (u64, u64)>,
    census: BTreeMap<(&'static str, &'static str), (u64, u64)>,
    budget: u64,
    root: u64,
    request_depth: usize,
    charge_depth: usize,
    shared_full_insert: bool,
}
thread_local! { static COW_TRACE: std::cell::RefCell<CowTraceState> = std::cell::RefCell::new(CowTraceState::default()); }
#[cfg(test)]
thread_local! { static COW_TRACE_FOR_TEST: std::cell::Cell<Option<bool>> = const { std::cell::Cell::new(None) }; }
fn cow_trace_enabled() -> bool {
    #[cfg(test)]
    if let Some(enabled) = COW_TRACE_FOR_TEST.with(|enabled| enabled.get()) { return enabled }
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os("SUSS_HIR_COW_TRACE").is_some())
}
pub(crate) struct CowTraceScope(bool);
impl CowTraceScope {
    pub(crate) fn new(label: &'static str) -> Self {
        let enabled = cow_trace_enabled();
        if enabled { COW_TRACE.with(|state| state.borrow_mut().stack.push(label)); }
        Self(enabled)
    }
}
impl Drop for CowTraceScope {
    fn drop(&mut self) {
        if self.0 { COW_TRACE.with(|state| { state.borrow_mut().stack.pop(); }); }
    }
}
/// A trace region follows the real ExpansionRequest lifetime. Only a root
/// clears counters; nested requests retain earlier reader/dependency work.
pub(crate) struct CowTraceRequest(bool);
impl CowTraceRequest {
    pub(crate) fn new(budget: u64) -> Self {
        let enabled = cow_trace_enabled();
        if enabled { COW_TRACE.with(|state| {
            let mut state = state.borrow_mut();
            if state.request_depth == 0 {
                state.totals.clear(); state.events.clear(); state.census.clear(); state.budget = budget;
                for (name, bytes) in cow_trace_layouts() {
                    state.census.insert(("layout", name), (1, bytes as u64));
                }
                state.root = state.root.wrapping_add(1);
            }
            state.request_depth += 1;
            eprintln!("HIR_COW_TRACE event=request_begin root={} depth={} available={} root_budget={} reset={}",
                state.root, state.request_depth, budget, state.budget, state.request_depth == 1);
        }); }
        Self(enabled)
    }
}
impl Drop for CowTraceRequest {
    fn drop(&mut self) {
        if !self.0 { return }
        COW_TRACE.with(|state| {
            let mut state = state.borrow_mut();
            state.request_depth -= 1;
            if state.request_depth != 0 { return }
            // Exactly one summary per root, including successful roots. Error
            // headers below describe attempted debits, not extra summaries.
            eprintln!("HIR_COW_TRACE event=root_end root={} budget={} charge_depth={}", state.root, state.budget, state.charge_depth);
            for (label, (calls, accepted, rejected)) in &state.totals {
                eprintln!("HIR_COW_TRACE bucket={label} calls={calls} accepted={accepted} rejected={rejected}");
            }
            for (label, (count, units)) in &state.events {
                eprintln!("HIR_COW_TRACE event={label} count={count} units={units}");
            }
            for ((family, kind), (count, bytes)) in &state.census {
                eprintln!("HIR_COW_TRACE census={family} kind={kind} count={count} bytes={bytes}");
            }
        });
    }
}
/// Forwarding requests still perform every original check/debit. Only their
/// diagnostic observations are suppressed: record once at the outermost call.
pub(crate) struct CowTraceCharge { enabled: bool, outermost: bool }
impl CowTraceCharge {
    pub(crate) fn new() -> Self {
        let enabled = cow_trace_enabled();
        let outermost = enabled && COW_TRACE.with(|state| {
            let mut state = state.borrow_mut();
            let outermost = state.charge_depth == 0;
            state.charge_depth += 1;
            outermost
        });
        Self { enabled, outermost }
    }
    pub(crate) fn record(&self, work: u64, result: &Result<(), Diagnostic>) {
        if self.outermost { cow_trace_charge(work, result); }
    }
}
impl Drop for CowTraceCharge {
    fn drop(&mut self) {
        if self.enabled { COW_TRACE.with(|state| state.borrow_mut().charge_depth -= 1); }
    }
}
fn cow_trace_charge(work: u64, result: &Result<(), Diagnostic>) {
    COW_TRACE.with(|state| {
        let mut state = state.borrow_mut();
        let leaf = state.stack.last().copied().unwrap_or("other_analysis");
        let label = if leaf == "form_copy" {
            let parents = &state.stack[..state.stack.len() - 1];
            let caller = if parents.is_empty() { "other_analysis".to_owned() } else { parents.join("/") };
            format!("form_copy:{caller}")
        } else if state.shared_full_insert && matches!(leaf, "map_copy" | "map_growth_prepare") {
            format!("{leaf}:shared_full")
        } else { leaf.to_owned() };
        let totals = state.totals.entry(label).or_default();
        totals.0 = totals.0.saturating_add(1);
        if result.is_ok() { totals.1 = totals.1.saturating_add(work); }
        else { totals.2 = totals.2.saturating_add(work); }
        if let Err(error) = result {
            // Rejected units are attempted work; a host may already have
            // debited before a forwarding wrapper rejects its own allowance.
            eprintln!("HIR_COW_TRACE event=charge_error root={} budget={} error={:?} message={:?}",
                state.root, state.budget, error.span, error.message);
        }
    });
}
// Tuple sizes describe the current field types, not a promised layout for a
// future boxed payload. The actual candidate type would need its own size check.
fn cow_trace_layouts() -> [(&'static str, usize); 15] {
    use std::mem::size_of;
    [
        ("Hir", size_of::<Hir>()), ("Expression", size_of::<Expression>()),
        ("ArcHeader", 2 * size_of::<usize>()), ("Parameter", size_of::<Parameter>()),
        ("Binding", size_of::<Binding>()), ("Global", size_of::<Global>()),
        ("OptionParameter", size_of::<Option<Parameter>>()), ("OptionGlobal", size_of::<Option<Global>>()),
        ("GeneralFunction_field_tuple", size_of::<(Vec<Method>, Vec<BindingId>, Option<Arc<Parameter>>, Option<Global>)>()),
        ("Definition_field_tuple", size_of::<(Global, DefinitionSyntax, Range<usize>, Option<Box<Hir>>, bool)>()),
        ("DefinitionSyntax", size_of::<DefinitionSyntax>()),
        ("DefinitionSyntaxPayload", size_of::<DefinitionSyntaxPayload>()),
        ("AsyncTry_field_tuple", size_of::<(Box<Hir>, Option<Box<Hir>>, Option<Box<Hir>>, Option<Parameter>)>()),
        ("GlobalOrFallback_field_tuple", size_of::<(Global, Global)>()),
        ("Function_field_tuple", size_of::<(Vec<Parameter>, Vec<BindingId>, Box<Hir>)>()),
    ]
}
fn cow_trace_expression_kind(kind: &Expression) -> &'static str {
    use Expression::*;
    match kind {
        Future { .. } => "Future", Await { .. } => "Await", AsyncTry { .. } => "AsyncTry",
        AsyncDynamicScope { .. } => "AsyncDynamicScope", Bitwise { .. } => "Bitwise",
        Comparison { .. } => "Comparison", NilTest(_) => "NilTest", Array { .. } => "Array",
        DynamicScope { .. } => "DynamicScope", Assign { .. } => "Assign", Throw(_) => "Throw",
        Try { .. } => "Try", Nominal { .. } => "Nominal", Literal(_) => "Literal", Local(_) => "Local",
        Global(_) => "Global", GlobalOrFallback { .. } => "GlobalOrFallback", GlobalCell(_) => "GlobalCell",
        Definition { .. } => "Definition", Let { .. } => "Let", Loop { .. } => "Loop",
        Recur { .. } => "Recur", If { .. } => "If", Do(_) => "Do", Function { .. } => "Function",
        GeneralFunction { .. } => "GeneralFunction", Call { .. } => "Call", Arithmetic { .. } => "Arithmetic",
    }
}
pub(super) fn cow_trace_expression(family: &'static str, kind: &Expression) {
    if !cow_trace_enabled() { return }
    let kind_name = cow_trace_expression_kind(kind);
    COW_TRACE.with(|state| {
        let mut state = state.borrow_mut();
        let value = state.census.entry((family, kind_name)).or_default();
        value.0 = value.0.saturating_add(1);
        // Counts are diagnostic observations, never extra debits. Only the
        // capture family denotes an actual Arc<Hir> backing allocation here.
        let bytes = if family == "capture_arc" { std::mem::size_of::<Hir>() + 2 * std::mem::size_of::<usize>() }
            else { std::mem::size_of::<Expression>() };
        value.1 = value.1.saturating_add(bytes as u64);
        // Field rows are counts only (bytes=0), not measured allocations. They
        // partition the same family's variant observations and must not be
        // summed into construction or capture counts a second time.
        let mut field = |label| {
            let value = state.census.entry((family, label)).or_default();
            value.0 = value.0.saturating_add(1);
        };
        match kind {
            Expression::GeneralFunction { self_binding, rest_class, .. } => {
                field(if self_binding.is_some() { "GeneralFunction.self_binding_some" } else { "GeneralFunction.self_binding_none" });
                field(if rest_class.is_some() { "GeneralFunction.rest_class_some" } else { "GeneralFunction.rest_class_none" });
            }
            Expression::AsyncTry { payload, .. } => {
                field(if payload.is_some() { "AsyncTry.payload_some" } else { "AsyncTry.payload_none" });
            }
            Expression::Definition { syntax, .. } => {
                field(if syntax.docstring().is_some() { "Definition.docstring_some" } else { "Definition.docstring_none" });
                field(if syntax.name_metadata().is_empty() { "Definition.name_metadata_empty" } else { "Definition.name_metadata_nonempty" });
                field(if syntax.has_payload() { "Definition.syntax_some" } else { "Definition.syntax_none" });
                field(match &syntax.0 {
                    DefinitionSyntaxOwner::Empty => "Definition.syntax_empty",
                    DefinitionSyntaxOwner::Captured(_) => "Definition.syntax_captured",
                    DefinitionSyntaxOwner::Owned(_) => "Definition.syntax_owned",
                });
            }
            _ => {}
        }
    });
}

fn cow_trace_event(label: &'static str, units: usize) {
    COW_TRACE.with(|state| {
        let mut state = state.borrow_mut();
        let value = state.events.entry(label).or_default();
        value.0 = value.0.saturating_add(1);
        value.1 = value.1.saturating_add(u64::try_from(units).unwrap_or(u64::MAX));
    });
}
/// No map scans/reference-count reads are added when tracing is disabled.
struct CowTraceMapInsert { enabled: bool, previous: bool, shared_full: bool }
impl CowTraceMapInsert {
    fn new<T>(map: &Arc<HashMap<String, Arc<T>>>, growing: bool) -> Self {
        let enabled = cow_trace_enabled();
        if !enabled { return Self { enabled, previous: false, shared_full: false } }
        let shared = Arc::strong_count(map) != 1 || Arc::weak_count(map) != 0;
        let shared_full = shared && growing;
        let previous = COW_TRACE.with(|state| {
            let mut state = state.borrow_mut();
            let previous = state.shared_full_insert;
            state.shared_full_insert = shared_full;
            previous
        });
        cow_trace_event(match (shared, growing) {
            (true, true) => "map_insert_shared_full",
            (true, false) => "map_insert_shared_nongrowing",
            (false, true) => "map_insert_private_full",
            (false, false) => "map_insert_private_nongrowing",
        }, 0);
        if shared_full {
            cow_trace_event("shared_full_old_len", map.len());
            cow_trace_event("shared_full_old_capacity", map.capacity());
        }
        Self { enabled, previous, shared_full }
    }
    fn copy_ready<T>(&self, staged: Option<&HashMap<String, Arc<T>>>) {
        if self.shared_full {
            if let Some(staged) = staged {
                cow_trace_event("shared_full_copy_completed", staged.capacity());
                if staged.capacity() != 0 { cow_trace_event("shared_full_nonempty_copy_table", staged.capacity()); }
            }
        }
    }
    fn growth_charge_capacity(&self, capacity: usize) {
        if self.shared_full { cow_trace_event("shared_full_growth_charge_capacity", capacity); }
    }
    fn growth_ready(&self, capacity: usize) {
        if self.shared_full { cow_trace_event("shared_full_growth_completed", capacity); }
    }
}
impl Drop for CowTraceMapInsert {
    fn drop(&mut self) {
        if self.enabled { COW_TRACE.with(|state| state.borrow_mut().shared_full_insert = self.previous); }
    }
}

type CowMeter<'a> = dyn FnMut(u64) -> Result<(), Diagnostic> + 'a;

fn cow_charge(meter: &mut CowMeter<'_>, count: usize, size: usize) -> Result<(), Diagnostic> {
    let units = count.checked_mul(size).and_then(|n| u64::try_from(n).ok())
        .ok_or_else(|| fail(0..0, "Source ownership work overflow"))?;
    meter(units)
}
fn cow_vector<T>(len: usize, meter: &mut CowMeter<'_>) -> Result<Vec<T>, Diagnostic> {
    cow_charge(meter, len, std::mem::size_of::<T>())?;
    let mut result = Vec::new();
    result.try_reserve_exact(len).map_err(|_| fail(0..0, "Unable to reserve source ownership vector"))?;
    Ok(result)
}
/// Vec and Arc slice have separate backing allocations and copy work.
/// Arc::from retains the established generic Arc OOM policy; its allocation and
/// copy are charged before conversion, and conversion is staged before publish.
fn cow_parent_scopes<T>(scopes: &[Arc<T>], meter: &mut CowMeter<'_>) -> Result<Arc<[Arc<T>]>, Diagnostic> {
    let _trace = CowTraceScope::new("function_parents");
    let mut parents = cow_vector(scopes.len(), meter)?;
    for scope in scopes { parents.push(scope.clone()); }
    cow_charge(meter, scopes.len(), std::mem::size_of::<Arc<T>>())?;
    cow_charge(meter, 2, std::mem::size_of::<usize>())?;
    Ok(parents.into())
}
fn cow_scope_growth<T>(scopes: &Vec<Arc<T>>, meter: &mut CowMeter<'_>) -> Result<Option<Vec<Arc<T>>>, Diagnostic> {
    let _trace = CowTraceScope::new("scope_growth");
    meter(1)?;
    if scopes.len() < scopes.capacity() { return Ok(None); }
    let target = scopes.capacity().max(1).checked_mul(2)
        .ok_or_else(|| fail(0..0, "Source scope capacity overflow"))?;
    let mut staged = cow_vector(target, meter)?;
    cow_charge(meter, scopes.len(), std::mem::size_of::<Arc<T>>())?;
    for scope in scopes { staged.push(scope.clone()); }
    Ok(Some(staged))
}
fn cow_copy<T: Copy>(items: &[T], meter: &mut CowMeter<'_>) -> Result<Vec<T>, Diagnostic> {
    let mut result = cow_vector(items.len(), meter)?;
    result.extend_from_slice(items);
    Ok(result)
}
fn cow_text(value: &str, meter: &mut CowMeter<'_>) -> Result<String, Diagnostic> {
    cow_charge(meter, value.len(), 1)?;
    let mut result = String::new();
    result.try_reserve_exact(value.len()).map_err(|_| fail(0..0, "Unable to reserve source ownership string"))?;
    result.push_str(value);
    Ok(result)
}
fn cow_forms(forms: &[Form], meter: &mut CowMeter<'_>) -> Result<Vec<Form>, Diagnostic> {
    let _trace = CowTraceScope::new("form_vector");
    let mut result = cow_vector(forms.len(), meter)?;
    for form in forms { result.push(super::super::clone_form_metered(form, meter)?); }
    Ok(result)
}
/// Capture syntax once; binding revisions retain this immutable payload owner.
fn cow_shared_form(form: &Form, meter: &mut CowMeter<'_>) -> Result<Arc<Form>, Diagnostic> {
    let copied = super::super::clone_form_metered(form, meter)?;
    cow_charge(meter, 1, std::mem::size_of::<Form>())?;
    cow_charge(meter, 2, std::mem::size_of::<usize>())?;
    Ok(Arc::new(copied))
}
// A borrowed API origin is copied once into the Analyzer's immutable owner.
// Text, line indexes, metadata cache and reader overlays are Arc-backed; only
// the optional PathBuf has variable-sized owned contents to copy here.
fn origin_value_work(origin: &super::super::SourceOrigin, meter: &mut CowMeter<'_>) -> Result<(), Diagnostic> {
    let _trace = CowTraceScope::new("origin_copy");
    cow_charge(meter, 1, std::mem::size_of::<super::super::SourceOrigin>())?;
    if let Some(path) = origin.path() {
        cow_charge(meter, path.as_os_str().as_encoded_bytes().len(), 1)?;
    }
    Ok(())
}
pub(super) fn capture_origin_owner(origin: Option<&super::super::SourceOrigin>, meter: &mut CowMeter<'_>) -> Result<Option<Arc<super::super::SourceOrigin>>, Diagnostic> {
    meter(1)?;
    let Some(origin) = origin else { return Ok(None) };
    origin_value_work(origin, meter)?;
    cow_charge(meter, 2, std::mem::size_of::<usize>())?;
    Ok(Some(Arc::new(origin.clone())))
}
fn cow_origin(origin: &Option<Arc<super::super::SourceOrigin>>, meter: &mut CowMeter<'_>) -> Result<Option<Arc<super::super::SourceOrigin>>, Diagnostic> {
    let _trace = CowTraceScope::new("origin_share");
    meter(1)?; // bounded shared-owner clone; no Origin struct or PathBuf copy
    Ok(origin.clone())
}
pub(super) fn derive_reader_origin(origin: Option<&super::super::SourceOrigin>, lowered: Form, reader: Form, meter: &mut CowMeter<'_>) -> Result<Arc<super::super::SourceOrigin>, Diagnostic> {
    let empty;
    let origin = if let Some(origin) = origin { origin } else {
        // Empty provenance has one line-index element and no source text.
        cow_charge(meter, 1, std::mem::size_of::<super::super::SourceOrigin>())?;
        cow_charge(meter, 1, std::mem::size_of::<usize>())?;
        empty = super::super::SourceOrigin::new("", None);
        &empty
    };
    origin_value_work(origin, meter)?; // with_reader_expansion copies this value
    cow_charge(meter, 2, std::mem::size_of::<usize>())?; // new Origin Arc header
    cow_charge(meter, 2, std::mem::size_of::<Form>())?; // moved reader correspondence
    cow_charge(meter, 2, std::mem::size_of::<usize>())?; // correspondence Arc header
    Ok(Arc::new(origin.with_reader_expansion(lowered, reader)))
}
fn cow_global(global: &Global, meter: &mut CowMeter<'_>) -> Result<Global, Diagnostic> {
    cow_charge(meter, global.namespace().len(), 1)?;
    cow_charge(meter, global.name().len(), 1)?;
    Ok(global.clone())
}
fn cow_parameter(parameter: &Parameter, meter: &mut CowMeter<'_>) -> Result<Parameter, Diagnostic> {
    Ok(Parameter { id: parameter.id, name: cow_text(&parameter.name, meter)?,
        metadata: cow_forms(&parameter.metadata, meter)?, span: parameter.span.clone() })
}
/// Stage all owned syntax copies before publishing its immutable owner. This
/// does not reuse owners between independent declarations, even for equal data.
pub(super) fn capture_definition_syntax(metadata: &[Form], docstring: Option<&[u16]>, meter: &mut CowMeter<'_>) -> Result<DefinitionSyntax, Diagnostic> {
    if metadata.is_empty() && docstring.is_none() { return Ok(DefinitionSyntax::default()); }
    let name_metadata = cow_forms(metadata, meter)?;
    let docstring = docstring.map(|units| cow_copy(units, meter)).transpose()?;
    cow_charge(meter, 1, std::mem::size_of::<DefinitionSyntaxPayload>())?;
    cow_charge(meter, 2, std::mem::size_of::<usize>())?;
    // Retain the established generic Arc allocation/OOM policy.
    Ok(DefinitionSyntax(DefinitionSyntaxOwner::Owned(Arc::new(DefinitionSyntaxPayload { name_metadata, docstring }))))
}
/// The caller supplies only its exact locally retained provisional revision,
/// never an environment re-query or completed HIR owner. Checking both state
/// fields prevents a completed initializer graph from entering this owner edge.
pub(super) fn capture_provisional_definition_syntax(owner: &Arc<super::super::resolve::DefinitionInfo>, meter: &mut CowMeter<'_>) -> Result<DefinitionSyntax, Diagnostic> {
    if owner.analysis_completed || owner.initializer.is_some() {
        meter(1)?;
        return Err(fail(owner.declaration.span.clone(), "Definition syntax requires an incomplete initializer-free revision"));
    }
    if owner.declaration.metadata.is_empty() && owner.docstring.is_none() {
        return Ok(DefinitionSyntax::default());
    }
    meter(1)?; // precharged Arc retain; no syntax copies or owner allocation
    Ok(DefinitionSyntax(DefinitionSyntaxOwner::Captured(owner.clone())))
}
fn cow_definition_syntax(syntax: &DefinitionSyntax, meter: &mut CowMeter<'_>) -> Result<DefinitionSyntax, Diagnostic> {
    if syntax.has_payload() { meter(1)?; }
    Ok(syntax.clone())
}
/// Move the completed parameter into an immutable owner, after charging its
/// payload and Arc header. Nested text/metadata are moved, not copied. Arc::new
/// retains the existing generic Arc allocation/OOM policy.
pub(super) fn capture_self_parameter(parameter: Parameter, meter: &mut CowMeter<'_>) -> Result<Arc<Parameter>, Diagnostic> {
    cow_charge(meter, 1, std::mem::size_of::<Parameter>())?;
    cow_charge(meter, 2, std::mem::size_of::<usize>())?;
    Ok(Arc::new(parameter))
}
fn cow_self_parameter(parameter: &Option<Arc<Parameter>>, meter: &mut CowMeter<'_>) -> Result<Option<Arc<Parameter>>, Diagnostic> {
    if let Some(parameter) = parameter {
        meter(1)?; // bounded retain of the immutable parameter; no payload copy
        Ok(Some(parameter.clone()))
    } else {
        Ok(None)
    }
}
fn cow_parameters(parameters: &[Parameter], meter: &mut CowMeter<'_>) -> Result<Vec<Parameter>, Diagnostic> {
    let mut result = cow_vector(parameters.len(), meter)?;
    for parameter in parameters { result.push(cow_parameter(parameter, meter)?); }
    Ok(result)
}
fn cow_hirs(nodes: &[Hir], meter: &mut CowMeter<'_>) -> Result<Vec<Hir>, Diagnostic> {
    let mut result = cow_vector(nodes.len(), meter)?;
    for node in nodes { result.push(cow_hir(node, meter)?); }
    Ok(result)
}
fn cow_box(node: &Hir, meter: &mut CowMeter<'_>) -> Result<Box<Hir>, Diagnostic> {
    Ok(Box::new(cow_hir(node, meter)?))
}
fn cow_optional_box(node: &Option<Box<Hir>>, meter: &mut CowMeter<'_>) -> Result<Option<Box<Hir>>, Diagnostic> {
    node.as_deref().map(|node| cow_box(node, meter)).transpose()
}
fn cow_captures(ids: &[BindingId], meter: &mut CowMeter<'_>) -> Result<Vec<BindingId>, Diagnostic> {
    cow_copy(ids, meter)
}
fn cow_bindings(bindings: &[Binding], meter: &mut CowMeter<'_>) -> Result<Vec<Binding>, Diagnostic> {
    let mut result = cow_vector(bindings.len(), meter)?;
    for binding in bindings { result.push(Binding { id: binding.id,
        name: cow_text(&binding.name, meter)?, metadata: cow_forms(&binding.metadata, meter)?,
        span: binding.span.clone(), value: binding.value.clone() }); }
    Ok(result)
}
fn cow_dynamic(bindings: &[(Global, Hir)], meter: &mut CowMeter<'_>) -> Result<Vec<(Global, Hir)>, Diagnostic> {
    let mut result = cow_vector(bindings.len(), meter)?;
    for (global, node) in bindings { result.push((cow_global(global, meter)?, cow_hir(node, meter)?)); }
    Ok(result)
}
/// Clone owned executable access payloads, charging each node before copying it.
/// Source-analysis Arcs remain shared; no recursive derived HIR Clone is used.
fn cow_hir(node: &Hir, meter: &mut CowMeter<'_>) -> Result<Hir, Diagnostic> {
    let _trace = CowTraceScope::new("hir_copy");
    stacker::maybe_grow(256 * 1024, 4 * 1024 * 1024, || {
        cow_charge(meter, 1, std::mem::size_of::<Hir>())?;
        let metadata = cow_forms(&node.metadata, meter)?;
        use Expression::*;
        let kind = match &node.kind {
            Local(id) => Local(*id),
            Literal(value) => Literal(match value {
                super::Literal::String(units) => {
                    super::Literal::String(cow_copy(units, meter)?)
                }
                super::Literal::Nil => super::Literal::Nil,
                super::Literal::Undefined => super::Literal::Undefined,
                super::Literal::Bool(value) => super::Literal::Bool(*value),
                super::Literal::Number(value) => super::Literal::Number(*value),
            }),
            Global(global) => Global(cow_global(global, meter)?),
            GlobalCell(global) => GlobalCell(cow_global(global, meter)?),
            GlobalOrFallback { global, fallback } => GlobalOrFallback {
                global: cow_global(global, meter)?, fallback: cow_global(fallback, meter)? },
            Future { captures, body } => Future { captures: cow_captures(captures, meter)?, body: cow_box(body, meter)? },
            Await { value } => Await { value: cow_box(value, meter)? },
            AsyncTry { body, handler, cleanup, payload } => AsyncTry {
                body: cow_box(body, meter)?, handler: cow_optional_box(handler, meter)?,
                cleanup: cow_optional_box(cleanup, meter)?,
                payload: payload.as_ref().map(|p| cow_parameter(p, meter)).transpose()? },
            AsyncDynamicScope { bindings, body } => AsyncDynamicScope { bindings: cow_dynamic(bindings, meter)?, body: cow_box(body, meter)? },
            DynamicScope { bindings, body } => DynamicScope { bindings: cow_dynamic(bindings, meter)?, body: cow_box(body, meter)? },
            Bitwise { operation, arguments } => Bitwise { operation: *operation, arguments: cow_hirs(arguments, meter)? },
            Comparison { operation, arguments } => Comparison { operation: *operation, arguments: cow_hirs(arguments, meter)? },
            Array { operation, arguments } => Array { operation: *operation, arguments: cow_hirs(arguments, meter)? },
            Nominal { operation, arguments } => Nominal { operation: *operation, arguments: cow_hirs(arguments, meter)? },
            Arithmetic { operator, arguments } => Arithmetic { operator: *operator, arguments: cow_hirs(arguments, meter)? },
            NilTest(value) => NilTest(cow_box(value, meter)?),
            Throw(value) => Throw(cow_box(value, meter)?),
            Assign { global, value } => Assign { global: cow_global(global, meter)?, value: cow_box(value, meter)? },
            Try { regions } => Try { regions: [cow_box(&regions[0], meter)?, cow_box(&regions[1], meter)?, cow_box(&regions[2], meter)?] },
            Definition { global, syntax, name_span, initializer, once } => {
                let global = cow_global(global, meter)?;
                let syntax = cow_definition_syntax(syntax, meter)?;
                Definition { global, syntax, name_span: name_span.clone(),
                    initializer: cow_optional_box(initializer, meter)?, once: *once }
            }
            Let { bindings, body } => Let { bindings: cow_bindings(bindings, meter)?, body: cow_box(body, meter)? },
            Loop { target, bindings, body } => Loop { target: *target, bindings: cow_bindings(bindings, meter)?, body: cow_box(body, meter)? },
            Recur { target, arguments } => Recur { target: *target, arguments: cow_hirs(arguments, meter)? },
            If { condition, consequent, alternative } => If { condition: cow_box(condition, meter)?, consequent: cow_box(consequent, meter)?, alternative: cow_box(alternative, meter)? },
            Do(nodes) => Do(cow_hirs(nodes, meter)?),
            Function { parameters, captures, body } => Function { parameters: cow_parameters(parameters, meter)?, captures: cow_captures(captures, meter)?, body: cow_box(body, meter)? },
            GeneralFunction { methods, captures, self_binding, rest_class } => {
                let mut copied = cow_vector(methods.len(), meter)?;
                for method in methods { copied.push(Method { variadic: method.variadic,
                    parameters: cow_parameters(&method.parameters, meter)?, body: cow_box(&method.body, meter)? }); }
                let methods = copied;
                GeneralFunction { methods, captures: cow_captures(captures, meter)?,
                    self_binding: cow_self_parameter(self_binding, meter)?,
                    rest_class: rest_class.as_ref().map(|g| cow_global(g, meter)).transpose()? }
            }
            Call { callee, arguments } => Call { callee: cow_box(callee, meter)?, arguments: cow_hirs(arguments, meter)? },
        };
        cow_trace_expression("metered_clone", &kind);
        Ok(Hir { source: node.source.clone(), span: node.span.clone(), metadata, ty: node.ty, kind })
    })
}
fn cow_role(role: &SourceRole, meter: &mut CowMeter<'_>) -> Result<SourceRole, Diagnostic> {
    let _trace = CowTraceScope::new("role_copy");
    Ok(match role {
        SourceRole::CatchBinding { hidden, access } => SourceRole::CatchBinding {
            hidden: hidden.clone(), access: access.clone() },
        SourceRole::MethodThis { type_declaration, namespace, protocol_receiver, receiver_declaration, argument, access } => SourceRole::MethodThis {
            type_declaration: type_declaration.clone(),
            namespace: cow_text(namespace, meter)?, protocol_receiver: *protocol_receiver,
            receiver_declaration: receiver_declaration.clone(),
            argument: argument.clone(), access: access.clone() },
        SourceRole::Plain => SourceRole::Plain,
        SourceRole::FunctionName { variadic, methods } => SourceRole::FunctionName { variadic: *variadic, methods: methods.clone() },
        SourceRole::PrivateCatch { anchor } => SourceRole::PrivateCatch { anchor: anchor.clone() },
        SourceRole::MethodArgument { index, rest } => SourceRole::MethodArgument { index: *index, rest: *rest },
    })
}
fn cow_local(binding: &LocalBinding, meter: &mut CowMeter<'_>) -> Result<LocalBinding, Diagnostic> {
    let _trace = CowTraceScope::new("entry_copy");
    cow_charge(meter, 1, std::mem::size_of::<LocalBinding>())?;
    Ok(LocalBinding { identity: binding.identity.clone(), id: binding.id, ty: binding.ty,
        declaration: binding.declaration.clone(),
        origin: cow_origin(&binding.origin, meter)?, kind: binding.kind,
        declaration_context: binding.declaration_context, source_role: cow_role(&binding.source_role, meter)?,
        initializer: binding.initializer.clone(), shadow: binding.shadow.clone(), shadow_field: binding.shadow_field.clone() })
}
fn cow_table<T>(capacity: usize, meter: &mut CowMeter<'_>) -> Result<(), Diagnostic> {
    if capacity == 0 { return meter(1); }
    let buckets = capacity.checked_add(1).and_then(usize::checked_next_power_of_two)
        .ok_or_else(|| fail(0..0, "Source ownership capacity overflow"))?;
    let slot = std::mem::size_of::<(String, Arc<T>)>().checked_add(1)
        .ok_or_else(|| fail(0..0, "Source ownership capacity overflow"))?;
    cow_charge(meter, buckets, slot)?;
    // Conservatively include trailing control-group and alignment storage.
    cow_charge(meter, 4, std::mem::size_of::<usize>())
}
/// Stage a shared map copy; publish only once every key has been copied.
fn cow_map_copy<T>(map: &mut Arc<HashMap<String, Arc<T>>>, meter: &mut CowMeter<'_>) -> Result<Option<HashMap<String, Arc<T>>>, Diagnostic> {
    let _trace = CowTraceScope::new("map_copy");
    meter(1)?;
    if Arc::get_mut(map).is_some() { return Ok(None); }
    cow_table::<T>(map.capacity(), meter)?;
    cow_charge(meter, 1, std::mem::size_of::<HashMap<String, Arc<T>>>())?;
    let mut staged = HashMap::with_hasher(map.hasher().clone());
    staged.try_reserve(map.capacity()).map_err(|_| fail(0..0, "Unable to reserve source ownership map"))?;
    for (key, value) in map.iter() {
        meter(1)?;
        let key = cow_text(key, meter)?;
        staged.insert(key, value.clone());
    }
    Ok(Some(staged))
}
fn cow_map<T>(map: &mut Arc<HashMap<String, Arc<T>>>, meter: &mut CowMeter<'_>) -> Result<(), Diagnostic> {
    if let Some(staged) = cow_map_copy(map, meter)? { *map = Arc::new(staged); }
    Ok(())
}
fn cow_insert<T>(map: &mut Arc<HashMap<String, Arc<T>>>, name: &str, value: Arc<T>, meter: &mut CowMeter<'_>) -> Result<Option<Arc<T>>, Diagnostic> {
    let _trace = CowTraceScope::new("map_insert");
    // Charge lookup/hash/copy and potential growth before publishing the entry.
    meter(1)?;
    let key = cow_text(name, meter)?;
    let growing = !map.contains_key(name) && map.len() == map.capacity();
    let trace_insert = CowTraceMapInsert::new(map, growing);
    if growing {
        let _trace = CowTraceScope::new("map_growth_prepare");
        let capacity = map.capacity().checked_mul(2).and_then(|n| n.checked_add(4))
            .ok_or_else(|| fail(0..0, "Source ownership capacity overflow"))?;
        trace_insert.growth_charge_capacity(capacity);
        cow_table::<T>(capacity, meter)?;
        cow_table::<T>(map.capacity(), meter)?;
        // HashMap growth rehashes existing String keys, not just Arc slots.
        for existing in map.keys() {
            meter(1)?;
            cow_charge(meter, existing.len(), 1)?;
        }
    }
    let mut staged = cow_map_copy(map, meter)?;
    trace_insert.copy_ready(staged.as_ref());
    let previous = {
        let target = match staged.as_mut() {
            Some(staged) => staged,
            None => Arc::get_mut(map).expect("private source map"),
        };
        if growing {
            target.try_reserve(1).map_err(|_| fail(0..0, "Unable to grow source ownership map"))?;
            trace_insert.growth_ready(target.capacity());
        }
        target.insert(key, value)
    };
    if let Some(staged) = staged { *map = Arc::new(staged); }
    Ok(previous)
}
fn cow_remove<T>(map: &mut Arc<HashMap<String, Arc<T>>>, name: &str, meter: &mut CowMeter<'_>) -> Result<Option<Arc<T>>, Diagnostic> {
    let _trace = CowTraceScope::new("map_remove");
    meter(1)?;
    cow_charge(meter, name.len(), 1)?;
    cow_map(map, meter)?;
    Ok(Arc::get_mut(map).expect("private staged source map").remove(name))
}
fn local_revision_mut<'a>(locals: &'a mut SourceLocals, name: &str, meter: &mut CowMeter<'_>) -> Result<&'a mut LocalBinding, Diagnostic> {
    meter(1)?;
    cow_charge(meter, name.len(), 1)?;
    // A shared map copy retains entry Arcs, so the selected entry must also fork.
    let map_shared = Arc::strong_count(locals) != 1 || Arc::weak_count(locals) != 0;
    let entry = locals.get(name).expect("existing local");
    let entry_shared = Arc::strong_count(entry) != 1 || Arc::weak_count(entry) != 0;
    let replacement = if map_shared || entry_shared { Some(Arc::new(cow_local(entry, meter)?)) } else { None };
    cow_map(locals, meter)?;
    let entry = Arc::get_mut(locals).expect("private source map").get_mut(name).expect("existing local");
    if let Some(replacement) = replacement { *entry = replacement; }
    Ok(Arc::get_mut(entry).expect("private source entry"))
}

#[cfg(test)]
mod cow_trace_tests {
    use super::*;
    use crate::portable::{ExpansionHost, ExpansionRequest, NoExpansion};

    struct TraceTest {
        saved: CowTraceState,
        enabled: Option<bool>,
    }
    impl TraceTest {
        fn new(enabled: bool) -> Self {
            let saved = COW_TRACE.with(|state| std::mem::take(&mut *state.borrow_mut()));
            let previous = COW_TRACE_FOR_TEST.with(|state| state.replace(Some(enabled)));
            Self { saved, enabled: previous }
        }
    }
    impl Drop for TraceTest {
        fn drop(&mut self) {
            COW_TRACE.with(|state| *state.borrow_mut() = std::mem::take(&mut self.saved));
            COW_TRACE_FOR_TEST.with(|state| state.set(self.enabled));
        }
    }

    #[derive(Default)]
    struct Meter { accepted: u64 }
    impl ExpansionHost for Meter {
        fn expansion_work_fuel(&self) -> u64 { 100 - self.accepted }
        fn charge_expansion_work(&mut self, work: u64, span: Range<usize>) -> Result<(), Diagnostic> {
            if work == 13 { return Err(fail(span, "trace host rejection")) }
            if work == 99 { panic!("trace host unwind") }
            self.accepted += work;
            Ok(())
        }
        fn expand(&mut self, _: &Form, _: crate::portable::ExpansionContext<'_>) -> Result<Option<Form>, Diagnostic> { Ok(None) }
    }

    #[test]
    fn nested_forwarding_records_once_retains_earlier_work_and_recovers_after_error_and_unwind() {
        let _test = TraceTest::new(true);
        let mut host = Meter::default();
        {
            let mut root = ExpansionRequest::new(&mut host, 0..10).unwrap();
            root.charge_expansion_work(3, 1..2).unwrap();
            {
                let mut second = ExpansionRequest::new(&mut root, 0..10).unwrap();
                let mut third = ExpansionRequest::new(&mut second, 0..10).unwrap();
                let mut fourth = ExpansionRequest::new(&mut third, 0..10).unwrap();
                fourth.charge_expansion_work(7, 2..3).unwrap();
                COW_TRACE.with(|state| {
                    let state = state.borrow();
                    assert_eq!(state.budget, 100);
                    assert_eq!(state.request_depth, 4);
                    assert_eq!(state.totals["other_analysis"], (2, 10, 0));
                });
                let error = fourth.charge_expansion_work(13, 3..4).unwrap_err();
                assert_eq!(error.message, "trace host rejection");
                let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let _scope = CowTraceScope::new("unwinding_test");
                    let _ = fourth.charge_expansion_work(99, 4..5);
                }));
                assert!(panic.is_err());
                COW_TRACE.with(|state| {
                    assert_eq!(state.borrow().charge_depth, 0);
                    assert!(state.borrow().stack.is_empty());
                });
                fourth.charge_expansion_work(2, 5..6).unwrap();
            }
            COW_TRACE.with(|state| {
                let state = state.borrow();
                assert_eq!(state.request_depth, 1);
                assert_eq!(state.totals["other_analysis"], (4, 12, 13));
            });
        }
        assert_eq!(host.accepted, 12, "diagnostic suppression must not alter host debits");
        COW_TRACE.with(|state| {
            let state = state.borrow();
            assert_eq!(state.request_depth, 0);
            assert_eq!(state.charge_depth, 0);
            assert_eq!(state.root, 1);
        });
        let mut next = ExpansionRequest::new(&mut host, 0..10).unwrap();
        COW_TRACE.with(|state| {
            let state = state.borrow();
            assert_eq!(state.root, 2);
            assert_eq!(state.budget, 88);
            assert!(state.totals.is_empty());
        });
        next.charge_expansion_work(1, 6..7).unwrap();
        COW_TRACE.with(|state| assert_eq!(state.borrow().totals["other_analysis"], (1, 1, 0)));
    }

    #[test]
    fn hir_layout_and_variant_census_separates_real_capture_from_direct_and_clone_construction() {
        let _test = TraceTest::new(true);
        let _request = CowTraceRequest::new(100);
        let kind = Expression::GeneralFunction { methods: vec![], captures: vec![], self_binding: None, rest_class: None }.census_created();
        let original = Hir { source: None, span: 0..1, metadata: vec![], ty: Type::Value, kind };
        let cloned = original.clone();
        let metered = cow_hir(&original, &mut |_| Ok(())).unwrap();
        assert!(matches!(cloned.kind, Expression::GeneralFunction { .. }));
        assert!(matches!(metered.kind, Expression::GeneralFunction { .. }));
        // Exercise an actual capture through the native analyzer; no Session,
        // bootstrap restore, or guest execution is needed for this census test.
        let source = "(let* [x 1] x)";
        let forms = suss_reader::forms::read_forms(source).unwrap();
        let _analyzed = super::super::prepare(&forms, 0..source.len(), &Environment::default(), Phase::Runtime).unwrap();
        COW_TRACE.with(|state| {
            let state = state.borrow();
            assert_eq!(state.census[&("layout", "Hir")], (1, std::mem::size_of::<Hir>() as u64));
            assert_eq!(state.census[&("layout", "Expression")], (1, std::mem::size_of::<Expression>() as u64));
            assert_eq!(state.census[&("constructed_direct", "GeneralFunction")].0, 1);
            assert_eq!(state.census[&("derived_clone", "GeneralFunction")].0, 1);
            assert_eq!(state.census[&("metered_clone", "GeneralFunction")].0, 1);
            assert_eq!(state.census[&("capture_arc", "Literal")], (1, (std::mem::size_of::<Hir>() + 2 * std::mem::size_of::<usize>()) as u64));
        });
        let parameter = || Parameter { id: BindingId(9), name: "payload".into(), metadata: vec![], span: 0..1 };
        let body = || Box::new(Hir { source: None, span: 0..1, metadata: vec![], ty: Type::Nil,
            kind: Expression::Literal(Literal::Nil) });
        for kind in [
            Expression::GeneralFunction { methods: vec![], captures: vec![], self_binding: Some(Arc::new(parameter())), rest_class: None }.census_created(),
            Expression::AsyncTry { body: body(), handler: None, cleanup: None, payload: Some(parameter()) }.census_created(),
            Expression::AsyncTry { body: body(), handler: None, cleanup: None, payload: None }.census_created(),
        ] {
            let node = Hir { source: None, span: 0..1, metadata: vec![], ty: Type::Value, kind };
            let _cloned = node.clone();
            let _copied = cow_hir(&node, &mut |_| Ok(())).unwrap();
        }
        COW_TRACE.with(|state| {
            let state = state.borrow();
            for family in ["constructed_direct", "derived_clone", "metered_clone"] {
                for field in ["GeneralFunction.self_binding_none", "GeneralFunction.self_binding_some", "AsyncTry.payload_none", "AsyncTry.payload_some"] {
                    assert_eq!(state.census[&(family, field)], (1, 0));
                }
            }
        });
    }

    #[test]
    fn shared_full_insert_reports_the_actual_staged_copy_and_growth_without_changing_map_facts() {
        let _test = TraceTest::new(true);
        let mut host = NoExpansion;
        let mut request = ExpansionRequest::new(&mut host, 0..10).unwrap();
        let mut raw = HashMap::new();
        raw.try_reserve(3).unwrap();
        let old_capacity = raw.capacity();
        for index in 0..old_capacity { raw.insert(format!("key-{index}"), Arc::new(())); }
        let mut map = Arc::new(raw);
        let saved = map.clone();
        cow_insert(&mut map, "new-key", Arc::new(()), &mut |work| request.charge_expansion_work(work, 0..10)).unwrap();
        assert_eq!(saved.len(), old_capacity);
        assert!(!saved.contains_key("new-key"));
        assert_eq!(map.len(), old_capacity + 1);
        assert!(map.capacity() > old_capacity);
        COW_TRACE.with(|state| {
            let state = state.borrow();
            assert_eq!(state.events["map_insert_shared_full"], (1, 0));
            assert_eq!(state.events["shared_full_old_capacity"], (1, old_capacity as u64));
            assert_eq!(state.events["shared_full_copy_completed"], (1, old_capacity as u64));
            assert_eq!(state.events["shared_full_nonempty_copy_table"], (1, old_capacity as u64));
            assert_eq!(state.events["shared_full_growth_completed"], (1, map.capacity() as u64));
            assert!(state.totals["map_copy:shared_full"].1 > 0);
            assert!(state.totals["map_growth_prepare:shared_full"].1 > 0);
        });
    }

    #[test]
    fn disabled_trace_leaves_observer_state_untouched_and_ir_labels_are_distinct() {
        let _test = TraceTest::new(false);
        {
            let _request = CowTraceRequest::new(10);
            let _scope = CowTraceScope::new("ir_build");
            let charge = CowTraceCharge::new();
            charge.record(2, &Ok(()));
            let map: Arc<HashMap<String, Arc<()>>> = Arc::new(HashMap::new());
            let map_trace = CowTraceMapInsert::new(&map, true);
            map_trace.growth_charge_capacity(4);
            map_trace.growth_ready(3);
        }
        COW_TRACE.with(|state| {
            let state = state.borrow();
            assert_eq!(state.root, 0);
            assert_eq!(state.request_depth, 0);
            assert_eq!(state.charge_depth, 0);
            assert!(state.stack.is_empty() && state.totals.is_empty() && state.events.is_empty() && state.census.is_empty());
        });
        COW_TRACE_FOR_TEST.with(|enabled| enabled.set(Some(true)));
        let _request = CowTraceRequest::new(10);
        for (label, work) in [("ir_recurrence", 2), ("ir_build", 3)] {
            let _scope = CowTraceScope::new(label);
            let charge = CowTraceCharge::new();
            charge.record(work, &Ok(()));
        }
        COW_TRACE.with(|state| {
            let state = state.borrow();
            assert_eq!(state.totals["ir_recurrence"], (1, 2, 0));
            assert_eq!(state.totals["ir_build"], (1, 3, 0));
        });
    }
}

#[cfg(test)]
mod ownership_tests {
    use super::*;

    fn owned_syntax_owner(syntax: &DefinitionSyntax) -> &Arc<DefinitionSyntaxPayload> {
        let DefinitionSyntaxOwner::Owned(owner) = &syntax.0 else { panic!("owned syntax fallback") };
        owner
    }
    fn captured_syntax_owner(syntax: &DefinitionSyntax) -> &Arc<super::super::super::resolve::DefinitionInfo> {
        let DefinitionSyntaxOwner::Captured(owner) = &syntax.0 else { panic!("captured provisional syntax") };
        owner
    }
    fn definition_syntaxes(node: &Hir, result: &mut Vec<DefinitionSyntax>) {
        match &node.kind {
            Expression::Definition { syntax, initializer, .. } => {
                result.push(syntax.clone());
                if let Some(initializer) = initializer { definition_syntaxes(initializer, result); }
            }
            Expression::Function { body, .. } => definition_syntaxes(body, result),
            // Fixed function bodies retain their recurrence target in a Loop,
            // including functions with no parameters or explicit recur.
            Expression::Loop { bindings, body, .. } => {
                for binding in bindings { definition_syntaxes(&binding.value, result); }
                definition_syntaxes(body, result);
            }
            Expression::Do(nodes) => for node in nodes { definition_syntaxes(node, result); },
            _ => {}
        }
    }

    #[test]
    fn definition_syntax_captures_local_provisional_before_nested_same_name_replacement() {
        for phase in [Phase::Runtime, Phase::Macro] {
            let source = "(def ^{:marker [outer facts]} same \"outer\" (fn* [] (inspect-function-owner) (def ^{:marker [inner facts]} same \"inner\" (fn* [] (inspect-function-owner))) (inspect-function-owner)))";
            let forms = suss_reader::forms::read_forms(source).unwrap();
            let mut host = ObserveFunctionOwner::default();
            let (hir, environment) = prepare_with_expander(&forms, 0..source.len(), &Environment::default(), phase, &mut host).unwrap();
            assert_eq!(host.snapshots.len(), 3);
            let mut syntax = Vec::new();
            definition_syntaxes(&hir, &mut syntax);
            assert_eq!(syntax.len(), 2);
            for (index, expected_doc) in ["outer", "inner"].into_iter().enumerate() {
                let owner = captured_syntax_owner(&syntax[index]);
                assert!(Arc::ptr_eq(owner, &host.snapshots[index].1));
                assert!(!owner.analysis_completed && owner.initializer.is_none());
                assert_eq!(syntax[index].name_metadata().as_ptr(), owner.declaration.metadata.as_ptr());
                assert_eq!(syntax[index].docstring().unwrap(), expected_doc.encode_utf16().collect::<Vec<_>>().as_slice());
                assert_eq!(syntax[index].docstring().unwrap().as_ptr(), owner.docstring.as_ref().unwrap().as_ptr());
            }
            assert!(!Arc::ptr_eq(captured_syntax_owner(&syntax[0]), captured_syntax_owner(&syntax[1])));
            // The environment was replaced by the nested completed revision,
            // then the outer completed revision. Neither may become our owner.
            assert!(host.snapshots[2].1.analysis_completed);
            assert!(!Arc::ptr_eq(captured_syntax_owner(&syntax[0]), &host.snapshots[2].1));
            let scope = &host.snapshots[0].0[0];
            let global = function_owner_global(&environment, phase, &scope.namespace, "same");
            let completed = environment.shared_definition_info(&global).unwrap();
            assert!(completed.analysis_completed && completed.initializer.is_some());
            assert!(!Arc::ptr_eq(completed, captured_syntax_owner(&syntax[0])));
            for copy in [hir.clone(), cow_hir(&hir, &mut |_| Ok(())).unwrap()] {
                let mut copied = Vec::new();
                definition_syntaxes(&copy, &mut copied);
                assert_eq!(copied.len(), 2);
                for index in 0..2 {
                    assert!(Arc::ptr_eq(captured_syntax_owner(&syntax[index]), captured_syntax_owner(&copied[index])));
                }
            }
            // Capture cancellation neither retains nor publishes an owner;
            // a retry pays exactly one retain, without copying source payloads.
            let owner = captured_syntax_owner(&syntax[0]);
            let count = Arc::strong_count(owner);
            let rejected = capture_provisional_definition_syntax(owner, &mut |work| {
                assert_eq!(work, 1);
                Err(fail(11..29, "injected captured syntax cancellation"))
            });
            assert_eq!(rejected.unwrap_err().message, "injected captured syntax cancellation");
            assert_eq!(Arc::strong_count(owner), count);
            let mut charged = 0;
            let retry = capture_provisional_definition_syntax(owner, &mut |work| { charged += work; Ok(()) }).unwrap();
            assert_eq!(charged, 1);
            assert!(Arc::ptr_eq(owner, captured_syntax_owner(&retry)));
            for (analysis_completed, initializer) in [(true, None), (false, completed.initializer.clone())] {
                let mut invalid = (**owner).clone();
                invalid.analysis_completed = analysis_completed;
                invalid.initializer = initializer;
                let invalid = Arc::new(invalid);
                let result = capture_provisional_definition_syntax(&invalid, &mut |_| Ok(()));
                assert_eq!(result.unwrap_err().message, "Definition syntax requires an incomplete initializer-free revision");
                assert_eq!(Arc::strong_count(&invalid), 1);
            }
        }
    }

    #[test]
    fn preserved_declaration_syntax_owns_new_facts_and_empty_docstring_stays_captured() {
        for phase in [Phase::Runtime, Phase::Macro] {
            let source = "(def ^:original same \"old\" 1) (def empty \"\" 2)";
            let forms = suss_reader::forms::read_forms(source).unwrap();
            let (hir, environment) = prepare_with_expander(&forms, 0..source.len(), &Environment::default(), phase, &mut super::super::super::NoExpansion).unwrap();
            let mut syntax = Vec::new();
            definition_syntaxes(&hir, &mut syntax);
            assert_eq!(syntax.len(), 2);
            assert!(captured_syntax_owner(&syntax[1]).docstring.as_ref().unwrap().is_empty());
            assert_eq!(syntax[1].docstring(), Some(&[][..]));
            let global = function_owner_global(&environment, phase, environment.current_namespace(phase), "same");
            let prior = environment.shared_definition_info(&global).unwrap().clone();
            let preserved = "(def ^{:declared true :new [facts]} same)";
            let forms = suss_reader::forms::read_forms(preserved).unwrap();
            let Kind::List(parts) = &forms[0].kind else { unreachable!() };
            let mut owners = Vec::new();
            for _ in 0..2 {
                let (hir, revised) = prepare_with_expander(&forms, 0..preserved.len(), &environment, phase, &mut super::super::super::NoExpansion).unwrap();
                assert!(Arc::ptr_eq(&prior, revised.shared_definition_info(&global).unwrap()));
                let mut current = Vec::new();
                definition_syntaxes(&hir, &mut current);
                assert_eq!(current.len(), 1);
                assert_eq!(format!("{:?}", current[0].name_metadata()), format!("{:?}", parts[1].metadata));
                assert_eq!(current[0].docstring(), None);
                owners.push(owned_syntax_owner(&current[0]).clone());
            }
            assert!(!Arc::ptr_eq(&owners[0], &owners[1]));
            assert_eq!(prior.docstring.as_deref(), Some(&[111u16, 108, 100][..]));
        }
    }

    #[test]
    fn definition_syntax_keeps_presence_exact_facts_and_independent_owners_through_hir_copies() {
        let input = suss_reader::forms::read_forms("^{:tag [nested facts] :line 91} declaration").unwrap();
        let metadata = &input[0].metadata;
        assert!(!metadata.is_empty());
        let mut environment = Environment::default();
        let global = environment.declare_cell(Phase::Runtime, "syntax.owner", "definition").unwrap();
        let empty: &[u16] = &[];
        let text: &[u16] = &[100, 0xd800, 99]; // preserve raw UTF16, including a lone surrogate
        for metadata in [&[][..], metadata.as_slice()] {
            for docstring in [None, Some(empty), Some(text)] {
                let syntax = capture_definition_syntax(metadata, docstring, &mut |_| Ok(())).unwrap();
                assert_eq!(format!("{:?}", syntax.name_metadata()), format!("{metadata:?}"));
                assert_eq!(syntax.docstring(), docstring);
                assert_eq!(syntax.has_payload(), !metadata.is_empty() || docstring.is_some());
                let independent = capture_definition_syntax(metadata, docstring, &mut |_| Ok(())).unwrap();
                if let DefinitionSyntaxOwner::Owned(owner) = &syntax.0 {
                    assert!(!Arc::ptr_eq(owner, owned_syntax_owner(&independent)));
                }
                let node = Hir { source: None, span: 11..29, metadata: vec![], ty: Type::Value,
                    kind: Expression::Definition { global: global.clone(), syntax,
                        name_span: input[0].span.clone(), initializer: None, once: true } };
                if let Expression::Definition { syntax: DefinitionSyntax(DefinitionSyntaxOwner::Owned(owner)), .. } = &node.kind {
                    let count = Arc::strong_count(owner);
                    let rejected = cow_hir(&node, &mut |work| {
                        if work == 1 { Err(fail(11..29, "injected HIR syntax retain cancellation")) }
                        else { Ok(()) }
                    });
                    assert_eq!(rejected.unwrap_err().message, "injected HIR syntax retain cancellation");
                    assert_eq!(Arc::strong_count(owner), count);
                }
                let mut remaining = (std::mem::size_of::<Hir>() + global.namespace().len()
                    + global.name().len() + usize::from(!metadata.is_empty() || docstring.is_some())) as u64;
                let copied = cow_hir(&node, &mut |work| {
                    remaining = remaining.checked_sub(work).ok_or_else(|| fail(11..29, "definition copy budget exhausted"))?;
                    Ok(())
                }).unwrap();
                assert_eq!(remaining, 0);
                let cloned = node.clone();
                let Expression::Definition { syntax: original, .. } = &node.kind else { unreachable!() };
                for retained in [&copied, &cloned] {
                    let Expression::Definition { syntax, name_span, initializer, once, .. } = &retained.kind else { panic!("definition") };
                    assert_eq!(name_span, &input[0].span);
                    assert!(initializer.is_none() && *once);
                    assert_eq!(format!("{:?}", syntax.name_metadata()), format!("{metadata:?}"));
                    assert_eq!(syntax.docstring(), docstring);
                    if let DefinitionSyntaxOwner::Owned(owner) = &original.0 {
                        assert!(Arc::ptr_eq(owner, owned_syntax_owner(&syntax)));
                    } else { assert!(!syntax.has_payload()); }
                }
            }
        }
        let absent = capture_definition_syntax(&[], None, &mut |_| panic!("no syntax allocation")).unwrap();
        assert!(!cow_definition_syntax(&absent, &mut |_| panic!("no owner to retain")).unwrap().has_payload());
    }

    #[test]
    fn definition_syntax_copy_allocation_and_retain_failures_keep_prior_owner_and_retry() {
        let input = suss_reader::forms::read_forms("^{:tag [nested facts] :line 91} declaration").unwrap();
        let metadata = &input[0].metadata;
        let before = format!("{metadata:?}");
        let docstring = Some(&[100u16, 111, 99][..]);
        let mut charges = Vec::new();
        let prior = capture_definition_syntax(metadata, docstring, &mut |work| { charges.push(work); Ok(()) }).unwrap();
        assert_eq!(&charges[charges.len()-2..], &[
            std::mem::size_of::<DefinitionSyntaxPayload>() as u64,
            (2 * std::mem::size_of::<usize>()) as u64]);
        for reject in 0..charges.len() {
            let mut published = prior.clone();
            let mut calls = 0;
            let result = capture_definition_syntax(metadata, docstring, &mut |_| {
                let current = calls;
                calls += 1;
                if current == reject { Err(fail(11..29, "injected syntax cancellation")) }
                else { Ok(()) }
            });
            match result {
                Ok(syntax) => { published = syntax; panic!("unexpected publication: {published:?}"); }
                Err(error) => assert_eq!(error.message, "injected syntax cancellation"),
            }
            assert_eq!(calls, reject + 1);
            assert!(Arc::ptr_eq(owned_syntax_owner(&published), owned_syntax_owner(&prior)));
            assert_eq!(format!("{metadata:?}"), before);
            let retry = capture_definition_syntax(metadata, docstring, &mut |_| Ok(())).unwrap();
            assert!(!Arc::ptr_eq(owned_syntax_owner(&retry), owned_syntax_owner(&prior)));
            assert_eq!(format!("{:?}", retry.name_metadata()), before);
            assert_eq!(retry.docstring(), docstring);
        }
        let count = Arc::strong_count(owned_syntax_owner(&prior));
        let result = cow_definition_syntax(&prior, &mut |work| {
            assert_eq!(work, 1);
            Err(fail(11..29, "injected syntax retain cancellation"))
        });
        assert_eq!(result.unwrap_err().message, "injected syntax retain cancellation");
        assert_eq!(Arc::strong_count(owned_syntax_owner(&prior)), count);
        let mut charged = 0;
        let retry = cow_definition_syntax(&prior, &mut |work| { charged += work; Ok(()) }).unwrap();
        assert_eq!(charged, 1);
        assert!(Arc::ptr_eq(owned_syntax_owner(&retry), owned_syntax_owner(&prior)));
    }

    #[test]
    fn self_parameter_owner_prepays_allocation_and_hir_copies_retain_exact_payload() {
        let parameter = || Parameter {
            id: BindingId(7),
            name: "recursive-name".repeat(1024),
            metadata: suss_reader::forms::read_forms("{:tag [original nested facts]}").unwrap(),
            span: 11..29,
        };
        // Either allocation charge can fail before any owner is published.
        for reject in 0..2 {
            let mut calls = 0;
            let mut published = None;
            let result = capture_self_parameter(parameter(), &mut |_| {
                let current = calls;
                calls += 1;
                if current == reject { Err(fail(11..29, "injected self owner cancellation")) }
                else { Ok(()) }
            });
            match result {
                Ok(owner) => published = Some(owner),
                Err(error) => {
                    assert_eq!(error.span, 11..29);
                    assert_eq!(error.message, "injected self owner cancellation");
                }
            }
            assert!(published.is_none());
            assert_eq!(calls, reject + 1);
        }
        let value = parameter();
        let name_storage = value.name.as_ptr();
        let metadata_storage = value.metadata.as_ptr();
        let mut charges = Vec::new();
        let owner = capture_self_parameter(value, &mut |work| { charges.push(work); Ok(()) }).unwrap();
        assert_eq!(charges, vec![std::mem::size_of::<Parameter>() as u64,
            (2 * std::mem::size_of::<usize>()) as u64]);
        assert_eq!(owner.name.as_ptr(), name_storage);
        assert_eq!(owner.metadata.as_ptr(), metadata_storage);
        let node = Hir { source: None, span: 11..29, metadata: vec![], ty: Type::Value,
            kind: Expression::GeneralFunction { methods: vec![], captures: vec![],
                self_binding: Some(owner.clone()), rest_class: None } };
        let initial_count = Arc::strong_count(&owner);
        let rejected = cow_hir(&node, &mut |work| {
            if work == 1 { Err(fail(11..29, "injected retain cancellation")) }
            else { Ok(()) }
        });
        assert!(rejected.is_err());
        assert_eq!(Arc::strong_count(&owner), initial_count);
        // A retry fits node-copy plus one retain, independent of the large
        // parameter text/metadata. No deep Parameter copy is charged or made.
        let mut remaining = (std::mem::size_of::<Hir>() + 1) as u64;
        let copied = cow_hir(&node, &mut |work| {
            remaining = remaining.checked_sub(work).ok_or_else(|| fail(11..29, "copy budget exhausted"))?;
            Ok(())
        }).unwrap();
        assert_eq!(remaining, 0);
        let cloned = node.clone();
        for retained in [&copied, &cloned] {
            let Expression::GeneralFunction { self_binding: Some(binding), .. } = &retained.kind else { panic!("named function") };
            assert!(Arc::ptr_eq(binding, &owner));
            assert_eq!(binding.span, 11..29);
            assert_eq!(binding.id, BindingId(7));
            assert_eq!(binding.metadata.as_ptr(), metadata_storage);
        }
        // Anonymous functions add neither an owner nor a retain charge.
        assert!(cow_self_parameter(&None, &mut |_| panic!("no owner to retain")).unwrap().is_none());
    }

    #[derive(Default)]
    struct Observe(Vec<(SourceLocals, SourceFields)>);
    impl crate::portable::ExpansionHost for Observe {
        fn expand(
            &mut self,
            form: &Form,
            context: crate::portable::ExpansionContext<'_>,
        ) -> Result<Option<Form>, Diagnostic> {
            if matches!(&form.kind, Kind::List(items)
                if matches!(items.first().map(|item| &item.kind), Some(Kind::Symbol(name))
                    if name.namespace.is_none() && name.name == "fail-owner"))
            {
                return Err(fail(form.span.clone(), "injected owner failure"));
            }
            if matches!(&form.kind, Kind::List(items)
                if matches!(items.first().map(|item| &item.kind), Some(Kind::Symbol(name))
                    if name.namespace.is_none() && name.name == "inspect-owner"))
            {
                self.0.push((context.locals.clone(), context.fields.clone()));
                return Ok(Some(Form { kind: Kind::Nil, span: form.span.clone(), metadata: vec![] }));
            }
            Ok(None)
        }
    }

    fn function_owner_global(environment: &Environment, phase: Phase, namespace: &str, name: &str) -> Global {
        environment.resolve(phase, &suss_reader::Symbol {
            namespace: Some(namespace.to_owned()), name: name.to_owned(),
        }, 0..0).expect("existing definition binding").global().clone()
    }

    #[derive(Default)]
    struct ObserveFunctionOwner {
        snapshots: Vec<(Vec<Arc<FunctionScope>>, Arc<super::super::super::resolve::DefinitionInfo>)>,
        replace_function_once: bool,
    }
    impl crate::portable::ExpansionHost for ObserveFunctionOwner {
        fn expand(&mut self, form: &Form, context: crate::portable::ExpansionContext<'_>) -> Result<Option<Form>, Diagnostic> {
            let Kind::List(items) = &form.kind else { return Ok(None) };
            let Some(Form { kind: Kind::Symbol(name), .. }) = items.first() else { return Ok(None) };
            if self.replace_function_once && matches!(name.name.as_str(), "fn" | "fn*") {
                self.replace_function_once = false;
                return Ok(Some(form.clone())); // equal syntax is a distinct input
            }
            if name.name == "fail-function-owner" {
                return Err(fail(form.span.clone(), "injected function owner failure"));
            }
            if name.name == "inspect-function-owner" {
                let scope = context.function_scopes.last().expect("named function scope");
                let Kind::Symbol(name) = &scope.declaration.kind else { unreachable!() };
                let global = function_owner_global(context.environment, context.phase, &scope.namespace, &name.name);
                let current = context.environment.shared_definition_info(&global).expect("published provisional definition").clone();
                self.snapshots.push((context.function_scopes.to_vec(), current));
                return Ok(Some(Form { kind: Kind::Nil, span: form.span.clone(), metadata: vec![] }));
            }
            Ok(None)
        }
    }

    #[test]
    fn direct_definition_function_retains_actual_provisional_owner_and_nested_def_restores_parent() {
        for phase in [Phase::Runtime, Phase::Macro] {
            // A nested definition deliberately replaces the SAME global. The
            // resumed outer function must keep its earlier provisional owner.
            let source = "(def outer (fn [x] (inspect-function-owner) (def outer (fn* [] (inspect-function-owner))) (inspect-function-owner)))";
            let mut host = ObserveFunctionOwner::default();
            let forms = suss_reader::forms::read_forms(source).unwrap();
            let (_, environment) = prepare_with_expander(&forms, 0..source.len(), &Environment::default(), phase, &mut host).unwrap();
            assert_eq!(host.snapshots.len(), 3);
            let outer = &host.snapshots[0].0[0];
            let FunctionFormOwner::DefinitionInitializer(owner) = &outer.function_form.owner else { panic!("direct bootstrap fn must reuse") };
            assert!(Arc::ptr_eq(owner, &host.snapshots[0].1));
            assert!(std::ptr::eq(outer.function_form.as_form(), owner.initializer_form.as_ref().unwrap()));
            let Kind::List(definition_items) = &forms[0].kind else { unreachable!() };
            assert_eq!(format!("{:?}", outer.function_form.as_form()), format!("{:?}", &definition_items[2]));
            assert!(!owner.analysis_completed);
            assert!(owner.initializer.is_none());
            let nested = host.snapshots[1].0.last().unwrap();
            let FunctionFormOwner::DefinitionInitializer(nested_owner) = &nested.function_form.owner else { panic!("nested definition owns its exact input") };
            assert!(Arc::ptr_eq(nested_owner, &host.snapshots[1].1));
            assert!(!Arc::ptr_eq(owner, nested_owner));
            assert!(Arc::ptr_eq(outer, &host.snapshots[2].0[0]));
            assert!(!Arc::ptr_eq(owner, &host.snapshots[2].1));
            let global = function_owner_global(&environment, phase, &outer.namespace, "outer");
            let completed = environment.shared_definition_info(&global).unwrap();
            assert!(completed.analysis_completed && completed.initializer.is_some());
            assert!(!Arc::ptr_eq(owner, completed));
            assert!(owner.initializer.is_none());
        }
    }

    #[test]
    fn replaced_equal_function_and_nested_function_use_owned_fallback_defonce_reuses_actual_owner() {
        for phase in [Phase::Runtime, Phase::Macro] {
            let source = "(def outer (fn* [] (inspect-function-owner)))";
            let forms = suss_reader::forms::read_forms(source).unwrap();
            let mut generated = ObserveFunctionOwner { replace_function_once: true, ..Default::default() };
            prepare_with_expander(&forms, 0..source.len(), &Environment::default(), phase, &mut generated).unwrap();
            let scope = &generated.snapshots[0].0[0];
            assert!(matches!(scope.function_form.owner, FunctionFormOwner::Owned(_)));
            assert_eq!(scope.function_form.as_form().span, generated.snapshots[0].1.initializer_form.as_ref().unwrap().span);
            assert!(!std::ptr::eq(scope.function_form.as_form(), generated.snapshots[0].1.initializer_form.as_ref().unwrap()));

            // A named nested function has no definition-input authority, even
            // when its declaration has the same spelling as the outer def.
            let source = "(defonce outer (fn* [] (fn* outer [] (inspect-function-owner)) (inspect-function-owner)))";
            let forms = suss_reader::forms::read_forms(source).unwrap();
            let mut host = ObserveFunctionOwner::default();
            let (_, environment) = prepare_with_expander(&forms, 0..source.len(), &Environment::default(), phase, &mut host).unwrap();
            assert_eq!(host.snapshots.len(), 2);
            assert_eq!(host.snapshots[0].0.len(), 2);
            assert!(matches!(host.snapshots[0].0[1].function_form.owner, FunctionFormOwner::Owned(_)));
            let FunctionFormOwner::DefinitionInitializer(owner) = &host.snapshots[1].0[0].function_form.owner else { panic!("defonce actual initializer") };
            assert!(owner.once && owner.initializer.is_none());
            assert!(Arc::ptr_eq(owner, &host.snapshots[1].1));

            // Preserving a declared def has no initializer and must not lend
            // an earlier revision to a later standalone named function.
            let preserved = "(def ^:declared outer) (fn* outer [] (inspect-function-owner))";
            let forms = suss_reader::forms::read_forms(preserved).unwrap();
            let namespace = host.snapshots[1].0[0].namespace.clone();
            let mut host = ObserveFunctionOwner::default();
            let old = environment.shared_definition_info(&function_owner_global(&environment, phase, &namespace, "outer")).unwrap().clone();
            let (_, revised) = prepare_with_expander(&forms, 0..preserved.len(), &environment, phase, &mut host).unwrap();
            assert!(matches!(host.snapshots[0].0[0].function_form.owner, FunctionFormOwner::Owned(_)));
            assert!(Arc::ptr_eq(&old, revised.shared_definition_info(&function_owner_global(&revised, phase, &host.snapshots[0].0[0].namespace, "outer")).unwrap()));
        }
    }

    #[test]
    fn function_owner_capture_failure_keeps_prior_revision_and_equal_copy_cannot_reuse() {
        for phase in [Phase::Runtime, Phase::Macro] {
            let source = "(def outer (fn* [] (inspect-function-owner) (fail-function-owner)))";
            let forms = suss_reader::forms::read_forms(source).unwrap();
            let mut host = ObserveFunctionOwner::default();
            let error = prepare_with_expander(&forms, 0..source.len(), &Environment::default(), phase, &mut host).unwrap_err();
            assert_eq!(error.message, "injected function owner failure");
            assert_eq!(&source[error.span], "(fail-function-owner)");
            let scope = &host.snapshots[0].0[0];
            let FunctionFormOwner::DefinitionInitializer(owner) = &scope.function_form.owner else { panic!("retained failed analysis owner") };
            assert!(owner.initializer.is_none());
            let input = owner.initializer_form.as_ref().unwrap();
            let candidate = DefinitionInitializer { input, owner };
            let error = FunctionForm::capture(input, Some(&candidate), &mut |_| Err(fail(7..9, "capture cancelled"))).unwrap_err();
            assert_eq!(error.message, "capture cancelled");
            let captured = FunctionForm::capture(input, Some(&candidate), &mut |_| Ok(())).unwrap();
            let FunctionFormOwner::DefinitionInitializer(retry) = captured.owner else { unreachable!() };
            assert!(Arc::ptr_eq(owner, &retry));
            let equal = input.clone();
            let mut calls = 0;
            let fallback = FunctionForm::capture(&equal, Some(&candidate), &mut |_| { calls += 1; Ok(()) }).unwrap();
            assert!(matches!(fallback.owner, FunctionFormOwner::Owned(_)));
            // Every fallible clone/owner staging boundary rejects cleanly;
            // retained provisional syntax is unchanged and a retry succeeds.
            for stop in 0..calls {
                let mut current = 0;
                let error = FunctionForm::capture(&equal, Some(&candidate), &mut |_| {
                    let call = current; current += 1;
                    if call == stop { Err(fail(7..9, "fallback cancelled")) } else { Ok(()) }
                }).unwrap_err();
                assert_eq!(error.message, "fallback cancelled");
                assert!(owner.initializer.is_none());
            }
            assert!(matches!(FunctionForm::capture(&equal, Some(&candidate), &mut |_| Ok(())).unwrap().owner, FunctionFormOwner::Owned(_)));
            assert!(owner.initializer.is_none());
        }
    }

    #[test]
    fn lexical_capture_preserves_map_and_entry_revisions_and_restores_scope_owner() {
        let source = "(let* [x 1] (inspect-owner) (inspect-owner) (let* [y 2] (inspect-owner)) (inspect-owner))";
        for phase in [Phase::Runtime, Phase::Macro] {
            let mut host = Observe::default();
            crate::portable::prepare_fragment_forms_with_expander(
                suss_reader::forms::read_forms(source).unwrap(), 0..source.len(),
                &Environment::default(), phase, &mut host,
            ).unwrap();
            assert_eq!(host.0.len(), 4);
            let (entry, fields) = &host.0[0];
            assert!(Arc::ptr_eq(entry, &host.0[1].0));
            assert!(!Arc::ptr_eq(entry, &host.0[2].0));
            assert!(Arc::ptr_eq(&entry["x"], &host.0[2].0["x"]));
            assert!(Arc::ptr_eq(entry, &host.0[3].0));
            assert!(host.0.iter().all(|(_, other)| Arc::ptr_eq(fields, other)));

            // A fact revision keeps its declaration identity, but cannot reuse
            // either the old descriptor owner or its enclosing map revision.
            let mut changed = entry.clone();
            let binding = local_revision_mut(&mut changed, "x", &mut |_| Ok(())).unwrap();
            binding.source_role = SourceRole::FunctionName { variadic: false, methods: None };
            binding.declaration_context = crate::portable::AnalysisContext::Return;
            binding.id = BindingId(binding.id.0 + 1);
            assert!(!Arc::ptr_eq(entry, &changed));
            assert!(!Arc::ptr_eq(&entry["x"], &changed["x"]));
            assert!(Arc::ptr_eq(&entry["x"].identity, &changed["x"].identity));
            assert!(matches!(entry["x"].source_role, SourceRole::Plain));
            assert_ne!(entry["x"].id, changed["x"].id);
            assert!(Arc::ptr_eq(entry["x"].initializer.as_ref().unwrap(), changed["x"].initializer.as_ref().unwrap()));
            let resolved = SourceBinding::Local(entry["x"].clone());
            let SourceBinding::Local(resolved) = resolved else { unreachable!() };
            assert!(Arc::ptr_eq(&entry["x"], &resolved));
            changed = entry.clone();
            assert!(Arc::ptr_eq(entry, &changed));

            let mut field_scope = fields.clone();
            Arc::make_mut(&mut field_scope).insert("f".into(), Arc::new(FieldBinding {
                identity: Arc::new(()), declaration: (*entry["x"].declaration).clone(),
                origin: None, index: 0, mutable: false,
                access: (**entry["x"].initializer.as_ref().unwrap()).clone(),
            }));
            let saved_fields = field_scope.clone();
            let field = Arc::make_mut(Arc::make_mut(&mut field_scope).get_mut("f").unwrap());
            field.mutable = true;
            assert!(!Arc::ptr_eq(&saved_fields, &field_scope));
            assert!(!Arc::ptr_eq(&saved_fields["f"], &field_scope["f"]));
            assert!(Arc::ptr_eq(&saved_fields["f"].identity, &field_scope["f"].identity));
            assert!(!saved_fields["f"].mutable);
            // Same locals with a different fields owner is a different projection.
            assert!(Arc::ptr_eq(entry, &changed));
            field_scope = saved_fields.clone();
            assert!(Arc::ptr_eq(&field_scope, &saved_fields));
        }
    }
    fn copy_fixture() -> SourceLocals {
        let declaration = suss_reader::forms::read_forms("^{:doc \"owned declaration bytes\"} x").unwrap().remove(0);
        let leaf = Hir { source: None, span: 3..7, metadata: vec![declaration.clone()],
            ty: Type::String, kind: Expression::Literal(Literal::String(vec![65; 64])) };
        let access = Hir { source: None, span: 3..7, metadata: vec![], ty: Type::Value,
            kind: Expression::Call { callee: Box::new(leaf), arguments: vec![Hir {
                source: None, span: 3..7, metadata: vec![], ty: Type::Value,
                kind: Expression::Local(BindingId(0)),
            }] } };
        let declaration = Arc::new(declaration);
        let binding = Arc::new(LocalBinding {
            identity: Arc::new(()), id: BindingId(0), ty: Type::Value,
            declaration: declaration.clone(), origin: None, kind: LocalKind::Let,
            declaration_context: crate::portable::AnalysisContext::Expression,
            source_role: SourceRole::MethodThis {
                type_declaration: declaration.clone(), namespace: "owned.namespace".into(),
                protocol_receiver: true, receiver_declaration: declaration,
                argument: None, access: Arc::new(access),
            }, initializer: None, shadow: None, shadow_field: None,
        });
        Arc::new(HashMap::from([("x".into(), binding.clone()), ("untouched".into(), binding)]))
    }

    #[test]
    fn cow_budget_and_interrupt_failures_keep_captured_revision_and_retry_cleanly() {
        let saved = copy_fixture();
        let mut attempt = saved.clone();
        let mut charges = Vec::new();
        let copied = local_revision_mut(&mut attempt, "x", &mut |units| { charges.push(units); Ok(()) }).unwrap();
        assert_eq!(format!("{copied:?}"), format!("{:?}", saved["x"]));
        assert!(Arc::ptr_eq(&copied.declaration, &saved["x"].declaration));
        let SourceRole::MethodThis { type_declaration: old_type, receiver_declaration: old_receiver, access: old_access, .. } = &saved["x"].source_role else { unreachable!() };
        let SourceRole::MethodThis { type_declaration: new_type, receiver_declaration: new_receiver, access: new_access, .. } = &copied.source_role else { unreachable!() };
        assert!(Arc::ptr_eq(old_type, new_type));
        assert!(Arc::ptr_eq(old_receiver, new_receiver));
        assert!(Arc::ptr_eq(old_access, new_access));
        copied.id = BindingId(9);
        assert!(!Arc::ptr_eq(&attempt, &saved));
        assert!(Arc::ptr_eq(&attempt["untouched"], &saved["untouched"]));
        assert_eq!(saved["x"].id, BindingId(0));
        // Derive allowed rewrites from the actual fixture Form nodes, not from
        // clone errors. HIR/map charges retain the injected diagnostic span.
        let mut allowed_spans = vec![11..19];
        let mut forms = vec![saved["x"].declaration.as_ref()];
        let SourceRole::MethodThis { type_declaration, receiver_declaration, access, .. } = &saved["x"].source_role else { unreachable!() };
        forms.extend([type_declaration.as_ref(), receiver_declaration.as_ref()]);
        let Expression::Call { callee, arguments } = &access.kind else { unreachable!() };
        forms.extend(access.metadata.iter());
        forms.extend(callee.metadata.iter());
        for argument in arguments { forms.extend(argument.metadata.iter()); }
        while let Some(form) = forms.pop() {
            allowed_spans.push(form.span.clone());
            forms.extend(form.metadata.iter());
            match &form.kind {
                Kind::List(items) | Kind::Vector(items) | Kind::Map(items) | Kind::Set(items)
                | Kind::Conditional(items) | Kind::SplicingConditional(items) => forms.extend(items),
                Kind::Discard(item) => forms.push(item),
                Kind::Prefix { operator, target } => { forms.push(operator); forms.push(target); }
                _ => {}
            }
        }
        // Fail at every remaining descriptor-copy preparation boundary,
        // including partway through the staged map copy. Immutable Form/HIR
        // payloads are shared and therefore contribute no deep-copy boundaries.
        for message in ["budget exhausted", "interrupt requested"] {
            for fail_at in 0..charges.len() {
                let mut attempt = saved.clone();
                let mut call = 0;
                let result = local_revision_mut(&mut attempt, "x", &mut |_| {
                    let current = call; call += 1;
                    if current == fail_at { Err(fail(11..19, message)) } else { Ok(()) }
                });
                let error = result.unwrap_err();
                assert!(allowed_spans.contains(&error.span), "unexpected diagnostic span {:?} at {fail_at}", error.span);
                assert_eq!(error.message, message);
                assert!(Arc::ptr_eq(&attempt, &saved), "{message} at {fail_at}");
                assert!(Arc::ptr_eq(&attempt["x"], &saved["x"]));
            }
        }
        // A real finite allowance is debited before copying, with no refill.
        let mut remaining = charges.iter().copied().sum::<u64>() - 1;
        let mut attempt = saved.clone();
        assert!(local_revision_mut(&mut attempt, "x", &mut |units| {
            remaining = remaining.checked_sub(units).ok_or_else(|| fail(11..19, "budget exhausted"))?;
            Ok(())
        }).is_err());
        assert!(Arc::ptr_eq(&attempt, &saved));
        local_revision_mut(&mut attempt, "x", &mut |_| Ok(())).unwrap().id = BindingId(10);
        assert_eq!(saved["x"].id, BindingId(0));
        attempt = saved.clone();
        assert!(Arc::ptr_eq(&attempt, &saved));
    }

    #[test]
    fn cow_map_growth_and_removal_fail_before_publication_and_work_overflow_is_checked() {
        let saved = copy_fixture();
        let mut attempt = saved.clone();
        let mut calls = 0;
        cow_insert(&mut attempt, "new-owned-key", saved["x"].clone(), &mut |_| { calls += 1; Ok(()) }).unwrap();
        for fail_at in 0..calls {
            let mut attempt = saved.clone();
            let mut call = 0;
            assert!(cow_insert(&mut attempt, "new-owned-key", saved["x"].clone(), &mut |_| {
                let current = call; call += 1;
                if current == fail_at { Err(fail(0..0, "interrupt requested")) } else { Ok(()) }
            }).is_err());
            assert!(Arc::ptr_eq(&attempt, &saved));
            assert!(!attempt.contains_key("new-owned-key"));
        }
        let mut empty = SourceLocals::default();
        assert!(cow_insert(&mut empty, "x", saved["x"].clone(), &mut |units|
            if units > 1 { Err(fail(0..0, "budget exhausted")) } else { Ok(()) }
        ).is_err());
        assert!(empty.is_empty());
        let mut attempt = saved.clone();
        assert!(cow_remove(&mut attempt, "x", &mut |_| Err(fail(0..0, "interrupt requested"))).is_err());
        assert!(Arc::ptr_eq(&attempt, &saved));
        assert!(cow_charge(&mut |_| panic!("overflow must not reach host"), usize::MAX, 2).is_err());
        assert!(cow_table::<LocalBinding>(usize::MAX, &mut |_| panic!("overflow must not allocate")).is_err());
    }

    #[test]
    fn unique_local_revision_checks_meter_without_copying_owned_payload() {
        let mut locals = copy_fixture();
        Arc::get_mut(&mut locals).unwrap().remove("untouched");
        let map_owner = Arc::as_ptr(&locals);
        let entry_owner = Arc::as_ptr(&locals["x"]);
        let mut units = 0;
        local_revision_mut(&mut locals, "x", &mut |work| { units += work; Ok(()) }).unwrap().id = BindingId(1);
        assert_eq!(Arc::as_ptr(&locals), map_owner);
        assert_eq!(Arc::as_ptr(&locals["x"]), entry_owner);
        assert_eq!(units, 3); // mutation bookkeeping, key bytes, map ownership check
        assert!(local_revision_mut(&mut locals, "x", &mut |_| Err(fail(0..0, "interrupt requested"))).is_err());
        assert_eq!(locals["x"].id, BindingId(1));
    }

    #[test]
    fn explicit_cow_vector_reservation_failure_is_recoverable() {
        assert!(cow_vector::<u8>(usize::MAX, &mut |_| Ok(())).is_err());
    }

    #[test]
    fn function_scope_parent_and_stack_staging_fail_before_publication() {
        let owner = Arc::new(());
        let mut scopes = Vec::new();
        scopes.push(owner.clone());
        while scopes.len() < scopes.capacity() { scopes.push(owner.clone()); }
        let original_ptr = scopes.as_ptr();
        let original_len = scopes.len();
        for parents in [true, false] {
            let mut count = 0;
            if parents { cow_parent_scopes(&scopes, &mut |_| { count += 1; Ok(()) }).unwrap(); }
            else { cow_scope_growth(&scopes, &mut |_| { count += 1; Ok(()) }).unwrap(); }
            for fail_at in 0..count {
                let mut call = 0;
                let mut meter = |_| {
                    let current = call; call += 1;
                    if current == fail_at { Err(fail(17..23, "scope staging interrupted")) } else { Ok(()) }
                };
                let error = if parents { cow_parent_scopes(&scopes, &mut meter).unwrap_err() }
                    else { cow_scope_growth(&scopes, &mut meter).unwrap_err() };
                assert_eq!(error.message, "scope staging interrupted");
                assert_eq!(scopes.as_ptr(), original_ptr);
                assert_eq!(scopes.len(), original_len);
                assert!(scopes.iter().all(|scope| Arc::ptr_eq(scope, &owner)));
            }
        }
        let parents = cow_parent_scopes(&scopes, &mut |_| Ok(())).unwrap();
        assert_eq!(parents.len(), scopes.len());
        assert!(parents.iter().all(|parent| Arc::ptr_eq(parent, &owner)));
        let mut staged = cow_scope_growth(&scopes, &mut |_| Ok(())).unwrap().unwrap();
        assert!(staged.capacity() > staged.len());
        staged.push(owner);
        assert_eq!(scopes.len(), original_len);
    }

    #[test]
    fn large_immutable_binding_payloads_survive_revision_without_copying_their_bytes() {
        let mut saved = copy_fixture();
        let payload = Arc::new(Form { kind: Kind::String(vec![65; 20_000]), span: 0..20_000, metadata: vec![] });
        let access = Arc::new(Hir { source: None, span: 0..20_000, metadata: vec![],
            ty: Type::String, kind: Expression::Literal(Literal::String(vec![66; 20_000])) });
        let binding = Arc::make_mut(Arc::get_mut(&mut saved).unwrap().get_mut("x").unwrap());
        binding.declaration = payload.clone();
        binding.source_role = SourceRole::MethodThis {
            type_declaration: payload.clone(), receiver_declaration: payload.clone(),
            namespace: "owned.namespace".into(), protocol_receiver: true,
            argument: None, access: access.clone(),
        };
        let mut changed = saved.clone();
        let mut remaining = 10_000u64; // far smaller than either immutable payload
        let revision = local_revision_mut(&mut changed, "x", &mut |units| {
            remaining = remaining.checked_sub(units).ok_or_else(|| fail(0..0, "budget exhausted"))?;
            Ok(())
        }).unwrap();
        assert!(Arc::ptr_eq(&revision.declaration, &payload));
        let SourceRole::MethodThis { type_declaration, receiver_declaration, access: revised_access, .. } = &revision.source_role else { unreachable!() };
        assert!(Arc::ptr_eq(type_declaration, &payload));
        assert!(Arc::ptr_eq(receiver_declaration, &payload));
        assert!(Arc::ptr_eq(revised_access, &access));
        revision.source_role = SourceRole::Plain;
        assert!(remaining > 0);
        assert!(!Arc::ptr_eq(&changed["x"], &saved["x"]));
        assert!(Arc::ptr_eq(&changed["x"].identity, &saved["x"].identity));
        assert!(matches!(saved["x"].source_role, SourceRole::MethodThis { .. }));
        assert!(Arc::ptr_eq(&saved["x"].declaration, &payload));
    }

    #[test]
    fn executable_and_source_binding_share_the_actual_initializer_owner() {
        let source = "(let* [x (fn* [a] a)] (inspect-owner) (let* [y 2] (inspect-owner)) (inspect-owner) x)";
        for phase in [Phase::Runtime, Phase::Macro] {
            let mut host = Observe::default();
            let forms = suss_reader::forms::read_forms(source).unwrap();
            let (hir, _) = prepare_with_expander(&forms, 0..source.len(), &Environment::default(), phase, &mut host).unwrap();
            let Expression::Do(items) = &hir.kind else { panic!("top-level body") };
            let Expression::Let { bindings, .. } = &items[0].kind else { panic!("actual lexical binding") };
            assert_eq!(host.0.len(), 3);
            assert_eq!(host.0.iter().map(|(locals, _)| locals.contains_key("y")).collect::<Vec<_>>(), [false, true, false]);
            assert!(Arc::ptr_eq(&host.0[0].0, &host.0[2].0));
            assert!(!Arc::ptr_eq(&host.0[0].0, &host.0[1].0));
            let initializer = host.0[0].0["x"].initializer.as_ref().unwrap();
            for (locals, _) in &host.0 { assert!(Arc::ptr_eq(&bindings[0].value, locals["x"].initializer.as_ref().unwrap())); }
            assert!(Arc::ptr_eq(&bindings[0].value, initializer));
            let copied_descriptor = bindings[0].clone();
            assert!(Arc::ptr_eq(&copied_descriptor.value, initializer));
            assert!(initializer.source.is_some());

            // A later expansion error cannot rewrite retained snapshots or
            // fabricate another initializer copy, and the final marker is not run.
            let failed_source = "(let* [x (fn* [a] a)] (inspect-owner) (let* [y 2] (inspect-owner) (fail-owner)) (inspect-owner) x)";
            let mut failed = Observe::default();
            let failed_forms = suss_reader::forms::read_forms(failed_source).unwrap();
            let error = prepare_with_expander(&failed_forms, 0..failed_source.len(), &Environment::default(), phase, &mut failed).unwrap_err();
            assert_eq!(error.message, "injected owner failure");
            assert_eq!(&failed_source[error.span], "(fail-owner)");
            assert_eq!(failed.0.len(), 2);
            assert!(!failed.0[0].0.contains_key("y"));
            assert!(failed.0[1].0.contains_key("y"));
            let first = failed.0[0].0["x"].initializer.as_ref().unwrap();
            let second = failed.0[1].0["x"].initializer.as_ref().unwrap();
            assert!(Arc::ptr_eq(first, second));
            assert!(Arc::ptr_eq(&failed.0[0].0["x"], &failed.0[1].0["x"]));
            assert!(first.source.is_some());
        }
    }

    #[test]
    fn catch_role_revision_preserves_payload_owners_and_isolates_fact_changes() {
        let mut saved = copy_fixture();
        let hidden = saved["untouched"].clone();
        let SourceRole::MethodThis { access, .. } = &saved["x"].source_role else { unreachable!() };
        let access = access.clone();
        let binding = Arc::make_mut(Arc::get_mut(&mut saved).unwrap().get_mut("x").unwrap());
        binding.source_role = SourceRole::CatchBinding { hidden: hidden.clone(), access: access.clone() };
        let mut changed = saved.clone();
        let revision = local_revision_mut(&mut changed, "x", &mut |_| Ok(())).unwrap();
        let SourceRole::CatchBinding { hidden: copied_hidden, access: copied_access } = &revision.source_role else { unreachable!() };
        assert!(Arc::ptr_eq(copied_hidden, &hidden));
        assert!(Arc::ptr_eq(copied_access, &access));
        revision.source_role = SourceRole::Plain;
        assert!(matches!(saved["x"].source_role, SourceRole::CatchBinding { .. }));
        assert!(!Arc::ptr_eq(&saved["x"], &changed["x"]));
        assert!(Arc::ptr_eq(&saved["x"].identity, &changed["x"].identity));
    }

    #[test]
    fn captured_origin_owner_is_shared_by_scopes_revisions_and_source_facts() {
        let source = "(let* [x (fn* local-function [a] a)] (inspect-owner) (let* [y x] (inspect-owner)) (inspect-owner))";
        let origin = crate::portable::SourceOrigin::new(source, Some(std::path::PathBuf::from("owned/source.cljc")));
        for phase in [Phase::Runtime, Phase::Macro] {
            let mut host = Observe::default();
            let forms = suss_reader::forms::read_forms(source).unwrap();
            prepare_with_origin(&forms, 0..source.len(), &Environment::default(), phase, &mut host, Some(&origin)).unwrap();
            assert_eq!(host.0.len(), 3);
            let first = host.0[0].0["x"].origin.as_ref().unwrap();
            assert_eq!(first.text(), source);
            assert_eq!(first.path(), origin.path());
            for (locals, _) in &host.0 {
                assert!(Arc::ptr_eq(first, locals["x"].origin.as_ref().unwrap()));
            }
            assert!(Arc::ptr_eq(first, host.0[1].0["y"].origin.as_ref().unwrap()));
            let initializer = host.0[0].0["x"].initializer.as_ref().unwrap();
            let source_fact = initializer.source.as_ref().unwrap();
            assert!(Arc::ptr_eq(first, source_fact.origin.as_ref().unwrap()));
            let scope = source_fact.callable.as_ref().unwrap().name.as_ref().unwrap();
            assert!(Arc::ptr_eq(first, scope.origin.as_ref().unwrap()));
            let mut revised = host.0[0].0.clone();
            local_revision_mut(&mut revised, "x", &mut |_| Ok(())).unwrap().id = BindingId(999);
            assert!(Arc::ptr_eq(first, revised["x"].origin.as_ref().unwrap()));
            assert_ne!(revised["x"].id, host.0[0].0["x"].id);
        }
        let mut calls = 0;
        let owner = capture_origin_owner(Some(&origin), &mut |_| { calls += 1; Ok(()) }).unwrap().unwrap();
        for stop in 0..calls {
            let mut call = 0;
            assert!(capture_origin_owner(Some(&origin), &mut |_| {
                let current = call; call += 1;
                if current == stop { Err(fail(7..13, "origin capture interrupted")) } else { Ok(()) }
            }).is_err());
            assert_eq!(owner.text(), source);
            assert_eq!(owner.path(), origin.path());
        }
        let lowered = suss_reader::forms::read_forms("lowered").unwrap().remove(0);
        let reader = suss_reader::forms::read_forms("reader").unwrap().remove(0);
        let derived = derive_reader_origin(Some(&owner), lowered.clone(), reader.clone(), &mut |_| Ok(())).unwrap();
        assert!(!Arc::ptr_eq(&owner, &derived));
        assert_eq!(derived.text(), owner.text());
        assert_eq!(derived.path(), owner.path());
        assert_eq!(derived.macro_form_data_metered(&lowered, &mut |_| Ok(())).unwrap(), reader);
        assert_eq!(owner.text(), source);
    }

}
