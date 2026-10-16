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

/// Named source function scope; a definition hint does not create a lexical ID.
#[derive(Debug, Clone)]
pub struct FunctionScope {
    /// Actual function syntax before any method body expansion or lowering.
    pub function_form: Form,
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
            super::super::clone_form_metered(function_form, &mut meter)?
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
        cow_charge(&mut |work| self.expander.charge_expansion_work(work, node.span.clone()), 1, std::mem::size_of::<Hir>())?;
        cow_charge(&mut |work| self.expander.charge_expansion_work(work, node.span.clone()), 2, std::mem::size_of::<usize>())?;
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
    budget: u64,
}
thread_local! { static COW_TRACE: std::cell::RefCell<CowTraceState> = std::cell::RefCell::new(CowTraceState::default()); }
fn cow_trace_enabled() -> bool {
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
pub(crate) fn cow_trace_reset(budget: u64) {
    if cow_trace_enabled() { COW_TRACE.with(|state| {
        let mut state = state.borrow_mut();
        state.totals.clear(); state.budget = budget;
    }); }
}
pub(crate) fn cow_trace_charge(work: u64, result: &Result<(), Diagnostic>) {
    if !cow_trace_enabled() { return; }
    COW_TRACE.with(|state| {
        let mut state = state.borrow_mut();
        let leaf = state.stack.last().copied().unwrap_or("other_analysis");
        let label = if leaf == "form_copy" {
            let parents = &state.stack[..state.stack.len() - 1];
            let caller = if parents.is_empty() { "other_analysis".to_owned() } else { parents.join("/") };
            format!("form_copy:{caller}")
        } else { leaf.to_owned() };
        let totals = state.totals.entry(label).or_default();
        totals.0 += 1;
        if result.is_ok() { totals.1 += work; } else { totals.2 += work; }
        if let Err(error) = result {
            eprintln!("HIR_COW_TRACE budget={} error={:?} message={:?}", state.budget, error.span, error.message);
            for (label, (calls, accepted, rejected)) in &state.totals {
                eprintln!("HIR_COW_TRACE bucket={label} calls={calls} accepted={accepted} rejected={rejected}");
            }
        }
    });
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
            Definition { global, name_metadata, name_span, docstring, initializer, once } => {
                let global = cow_global(global, meter)?;
                let name_metadata = cow_forms(name_metadata, meter)?;
                let docstring = docstring.as_ref().map(|units| {
                    cow_copy(units, meter)
                }).transpose()?;
                Definition { global, name_metadata, name_span: name_span.clone(), docstring,
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
                    self_binding: self_binding.as_ref().map(|p| cow_parameter(p, meter)).transpose()?,
                    rest_class: rest_class.as_ref().map(|g| cow_global(g, meter)).transpose()? }
            }
            Call { callee, arguments } => Call { callee: cow_box(callee, meter)?, arguments: cow_hirs(arguments, meter)? },
        };
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
    if growing {
        let capacity = map.capacity().checked_mul(2).and_then(|n| n.checked_add(4))
            .ok_or_else(|| fail(0..0, "Source ownership capacity overflow"))?;
        cow_table::<T>(capacity, meter)?;
        cow_table::<T>(map.capacity(), meter)?;
        // HashMap growth rehashes existing String keys, not just Arc slots.
        for existing in map.keys() {
            meter(1)?;
            cow_charge(meter, existing.len(), 1)?;
        }
    }
    let mut staged = cow_map_copy(map, meter)?;
    let previous = {
        let target = match staged.as_mut() {
            Some(staged) => staged,
            None => Arc::get_mut(map).expect("private source map"),
        };
        if growing { target.try_reserve(1).map_err(|_| fail(0..0, "Unable to grow source ownership map"))?; }
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
mod ownership_tests {
    use super::*;

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
