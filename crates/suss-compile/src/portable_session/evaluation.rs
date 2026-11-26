//! Native form-data compilation in the caller's Store and persistent catalog.
//! One callback is installed per Store. Its profile is refreshed before owned
//! entries; borrowed entries retain the caller's fuel, cancellation and roots.
use super::*;
use crate::{
    portable_macro_data::FormBridge,
    portable_macros::{CompiledMacros, MacroAuthority, MacroService, PreparedEvaluation},
};

mod macro_function;
mod macro_binding;
pub(super) mod function_pool;
mod math_context;
mod native_values;
pub(crate) use math_context::EvaluationMathContext;

#[derive(Clone)]
struct MacroMetadata {
    metadata: Option<OwnedRooted<AnyRef>>,
    enabled: bool,
}

#[derive(Default)]
pub(super) struct Evaluation {
    values: native_values::Values,
    value_service: Mutex<Option<crate::portable_macros::MacroServiceWeak>>,
    retired: AtomicBool,
    profile: Mutex<Option<Arc<ReentryProfile>>>,
    bridge: Mutex<Option<Arc<FormBridge>>>,
    external: Mutex<Option<MacroService>>,
    own: Mutex<Option<MacroAuthority>>,
    local: Mutex<Option<MacroAuthority>>,
    compiler_helpers: Mutex<BTreeMap<&'static str, OwnedRooted<AnyRef>>>,
    namespace: Mutex<Option<NamespaceAccess>>,
    input_base: Mutex<Option<OwnedRooted<AnyRef>>>,
    input_active: AtomicBool,
    var_class: Mutex<Option<OwnedRooted<AnyRef>>>,
    meta_fn_class: Mutex<Option<OwnedRooted<AnyRef>>>,
    metadata_lookup: Mutex<Option<OwnedRooted<AnyRef>>>,
    syntax_tests: Mutex<Vec<OwnedRooted<AnyRef>>>,
    macro_metadata: Mutex<BTreeMap<CellIdentity, MacroMetadata>>,
    exception_info: Mutex<Option<OwnedRooted<AnyRef>>>,
}

#[derive(Clone)]
struct NamespaceAccess {
    cell: Global,
    namespace: OwnedRooted<AnyRef>,
    symbol: OwnedRooted<AnyRef>,
    namespace_slot: OwnedRooted<AnyRef>,
}
impl NamespaceAccess {
    fn capture(session: &mut SessionHost<'_>) -> Result<Self, SessionError> {
        let cell = session.core_cell("*ns*")?;
        let mut descriptor = |name: &str| {
            let cell = session.core_cell(name)?;
            let runtime = session.runtime;
            let value = {
                let mut roots = RootScope::new(&mut session.store);
                let get = runtime.get_func(&mut roots, "binding-get").unwrap();
                let cell = cell.get(&mut roots);
                call(
                    &mut roots,
                    runtime,
                    get,
                    &[cell],
                    session.identity,
                    &session.handles,
                )?
            };
            let descriptor = session.data_descriptor(&value, true)?;
            Ok::<_, SessionError>(descriptor.object()?.clone())
        };
        let namespace = descriptor("Namespace")?;
        let symbol = descriptor("Symbol")?;
        let slot_cell = session.core_cell("namespace-slot")?;
        let namespace_slot = {
            let mut roots = RootScope::new(&mut session.store);
            let get = session.runtime.get_func(&mut roots, "binding-get").unwrap();
            let cell = slot_cell.get(&mut roots);
            call(
                &mut roots,
                session.runtime,
                get,
                &[cell],
                session.identity,
                &session.handles,
            )?
            .object()?
            .clone()
        };
        Ok(Self {
            cell,
            namespace,
            symbol,
            namespace_slot,
        })
    }
    fn aliases(
        &self,
        session: &mut SessionHost<'_>,
        bridge: &FormBridge,
    ) -> Result<BTreeMap<String, String>, SessionError> {
        // Validate the actual current Namespace and its canonical descriptor
        // before invoking the original private slot accessor retained at boot.
        self.read(session)?;
        let (namespace, accessor) = {
            let mut roots = RootScope::new(&mut session.store);
            let get = session.runtime.get_func(&mut roots, "binding-get").unwrap();
            let cell = self.cell.get(&mut roots);
            let namespace = call(
                &mut roots,
                session.runtime,
                get,
                &[cell],
                session.identity,
                &session.handles,
            )?;
            let accessor = Val::AnyRef(Some(self.namespace_slot.to_rooted(&mut roots)));
            let accessor = own(&mut roots, accessor, session.identity, &session.handles)?;
            (namespace, accessor)
        };
        let index = bridge.quote(
            session,
            suss_reader::forms::Form {
                span: 0..0,
                metadata: vec![],
                kind: suss_reader::forms::Kind::Long(0),
            },
        )?;
        let value = session.invoke(&accessor, &[&namespace, &index])?;
        let form = bridge.read(session, &value, 0..0)?;
        let suss_reader::forms::Kind::Map(items) = form.into_parts().2 else {
            return Err(wasmtime::Error::msg("Namespace aliases are not a map").into());
        };
        let mut aliases = BTreeMap::new();
        for pair in items.chunks_exact(2) {
            session.charge_host_work(1)?;
            let (suss_reader::forms::Kind::Symbol(alias), suss_reader::forms::Kind::Symbol(target)) =
                (&pair[0].kind, &pair[1].kind)
            else {
                return Err(wasmtime::Error::msg("Namespace alias entries must be symbols").into());
            };
            let text = |symbol: &suss_reader::Symbol| match &symbol.namespace {
                Some(namespace) => format!("{namespace}/{}", symbol.name),
                None => symbol.name.clone(),
            };
            let target = text(target);
            aliases.insert(
                text(alias),
                portable::resolve::canonical(&target).to_owned(),
            );
        }
        Ok(aliases)
    }
    fn read(&self, session: &mut SessionHost<'_>) -> Result<String, SessionError> {
        session.charge_host_work(16)?;
        let namespace = {
            let mut roots = RootScope::new(&mut session.store);
            let get = session.runtime.get_func(&mut roots, "binding-get").unwrap();
            let cell = self.cell.get(&mut roots);
            call(&mut roots, session.runtime, get, &[cell], session.identity, &session.handles)?
        };
        self.value_name(session, &namespace)
    }
    fn value_name(&self, session: &mut SessionHost<'_>, namespace: &SessionValue) -> Result<String, SessionError> {
        if let Some(name) = session.native_namespace_name(namespace)? { return Ok(name); }
        let mut roots = RootScope::new(&mut session.store);
        let namespace = namespace.val(&mut roots.as_context_mut());
        fn fields(
            store: &mut StoreContextMut<'_, ()>,
            value: Val,
            expected: &OwnedRooted<AnyRef>,
            count: u32,
        ) -> wasmtime::Result<wasmtime::Rooted<wasmtime::ArrayRef>> {
            fn reference(value: Val) -> wasmtime::Result<wasmtime::Rooted<AnyRef>> {
                match value {
                    Val::AnyRef(Some(value)) => Ok(value),
                    _ => Err(wasmtime::Error::msg(
                        "Namespace storage contains a non-reference",
                    )),
                }
            }
            let object = reference(value)?.as_struct(&*store)?.ok_or_else(|| {
                wasmtime::Error::msg("Current namespace has an invalid nominal shape")
            })?;
            if object.ty(&*store)?.fields().len() != 4 {
                return Err(wasmtime::Error::msg(
                    "Current namespace has invalid object storage",
                ));
            }
            let actual = reference(object.field(&mut *store, 0)?)?;
            let expected = expected.to_rooted(&mut *store);
            if !wasmtime::Rooted::ref_eq(&*store, &actual, &expected)? {
                return Err(wasmtime::Error::msg(
                    "Current namespace has a foreign descriptor",
                ));
            }
            let values = reference(object.field(&mut *store, 1)?)?
                .as_array(&*store)?
                .ok_or_else(|| wasmtime::Error::msg("Current namespace has invalid fields"))?;
            if values.len(&*store)? != count {
                return Err(wasmtime::Error::msg(
                    "Current namespace has an invalid field count",
                ));
            }
            Ok(values)
        }
        let data = fields(&mut roots.as_context_mut(), namespace, &self.namespace, 2)?;
        let name = data.get(&mut roots, 0)?;
        let data = fields(&mut roots.as_context_mut(), name, &self.symbol, 5)?;
        let name = data
            .get(&mut roots, 1)?
            .unwrap_anyref()
            .and_then(|value| value.as_array(&roots).ok().flatten())
            .ok_or_else(|| wasmtime::Error::msg("Namespace name is not a string"))?;
        if !matches!(name.ty(&roots)?.element_type(), wasmtime::StorageType::I16) {
            return Err(wasmtime::Error::msg("Namespace name is not UTF-16").into());
        }
        let length = name.len(&roots)?;
        SessionHost::consume_host_work(
            roots.as_context_mut(),
            u64::from(length) * 2 + 1,
            &session.interrupt,
        )?;
        let units = name
            .elems(&mut roots)?
            .map(|unit| unit.unwrap_i32() as u16)
            .collect::<Vec<_>>();
        String::from_utf16(&units).map_err(|error| wasmtime::Error::from(error).into())
    }
}
impl Evaluation {
    pub(super) fn clear(&self) {
        self.retired.store(true, Ordering::SeqCst);
        self.values.clear();
        registry_lock(&self.value_service).take();
        // Drop compiler services and roots outside the locks. Their teardown may
        // retire native requests in another isolated phase Store.
        let profile = registry_lock(&self.profile).take();
        let bridge = registry_lock(&self.bridge).take();
        let external = registry_lock(&self.external).take();
        let own = registry_lock(&self.own).take();
        let local = registry_lock(&self.local).take();
        let compiler_helpers = std::mem::take(&mut *registry_lock(&self.compiler_helpers));
        let namespace = registry_lock(&self.namespace).take();
        let input_base = registry_lock(&self.input_base).take();
        self.input_active.store(false, Ordering::SeqCst);
        let var_class = registry_lock(&self.var_class).take();
        let metadata_lookup = registry_lock(&self.metadata_lookup).take();
        let syntax_tests = std::mem::take(&mut *registry_lock(&self.syntax_tests));
        drop((
            profile,
            bridge,
            external,
            own,
            local,
            compiler_helpers,
            namespace,
            var_class,
            input_base,
            metadata_lookup,
            syntax_tests,
        ));
    }
    fn bridge(&self, view: &mut SessionHost<'_>) -> Result<Arc<FormBridge>, SessionError> {
        let existing = registry_lock(&self.bridge).clone();
        if let Some(bridge) = existing {
            return Ok(bridge);
        }
        let bridge = Arc::new(FormBridge::new(view)?);
        *registry_lock(&self.bridge) = Some(bridge.clone());
        Ok(bridge)
    }
    fn service(&self) -> Result<MacroService, SessionError> {
        let existing = registry_lock(&self.external).clone();
        if let Some(service) = existing {
            return Ok(service);
        }
        // A bare Session has no user macro host yet. Retain one bootstrap host
        // for this Session; never construct a fresh host for each eval input.
        let service = CompiledMacros::new()?.service();
        *registry_lock(&self.external) = Some(service.clone());
        Ok(service)
    }
    fn evaluate(
        &self,
        profile: &ReentryProfile,
        caller: StoreContextMut<'_, ()>,
        input: Val,
    ) -> Result<SessionValue, SessionError> {
        let mut view = profile.bind(caller);
        let _values = view.native_values_scope();
        view.charge_host_work(1)?;
        let value = {
            let mut roots = RootScope::new(&mut view.store);
            own(&mut roots, input, view.identity, &view.handles)?
        };
        // Already materialized non-syntax values need no macro serialization.
        // Use retained compiled predicates, so rebinding public core names cannot
        // change this compiler boundary. Nested constants still go through the
        // ordinary constant compiler and retain its Clojure rejection rules.
        let direct_literal = {
            let mut roots = RootScope::new(&mut view.store);
            let input = value.val(&mut roots.as_context_mut());
            let test = view.runtime.get_func(&mut roots, "evaluation-direct-literal-test")
                .ok_or_else(|| wasmtime::Error::msg("Missing evaluation literal classifier"))?;
            let mut result = [Val::I32(0)];
            test.call(&mut roots, &[input], &mut result)?;
            result[0].unwrap_i32() != 0
        };
        if direct_literal {
            let tests = registry_lock(&self.syntax_tests).clone();
            let mut syntax = false;
            for test in tests {
                let test = {
                    let mut roots = RootScope::new(&mut view.store);
                    let value = Val::AnyRef(Some(test.to_rooted(&mut roots)));
                    own(&mut roots, value, view.identity, &view.handles)?
                };
                let result = view.invoke(&test, &[&value])?;
                let enabled = view.inspect(&result, |mut store, value| {
                    Ok(sentinel_tag(&mut store, &value)? == Some(runtime_abi::TRUE as u32))
                })?;
                if enabled { syntax = true; break; }
            }
            if !syntax { return Ok(value); }
        }
        let bridge = self.bridge(&mut view)?;
        let form = view.read_native_evaluation(&bridge, &value, 0..0)?;
        // Clojure evaluates a scalar literal to the actual argument object.
        // Preserve its caller-owned root rather than reconstructing equal data.
        if matches!(
            &form.kind,
            suss_reader::forms::Kind::NativeConstant { .. }
                | suss_reader::forms::Kind::Nil
                | suss_reader::forms::Kind::Bool(_)
                | suss_reader::forms::Kind::Long(_)
                | suss_reader::forms::Kind::Number(_)
                | suss_reader::forms::Kind::Exact(_)
                | suss_reader::forms::Kind::String(_)
                | suss_reader::forms::Kind::Keyword(_)
                | suss_reader::forms::Kind::Uuid(_)
        ) {
            return Ok(value);
        }
        let authority = registry_lock(&self.own).clone();
        if let Some(authority) = authority {
            return authority.bind(view).evaluate_native_form(form);
        }
        let service = self.service()?;
        let existing = registry_lock(&self.local).clone();
        let authority = match existing {
            Some(authority) => authority,
            None => {
                let authority = MacroAuthority::native(&mut view, bridge, service.clone())?;
                *registry_lock(&self.local) = Some(authority.clone());
                authority
            }
        };
        authority.with_fallback(service).bind(view).evaluate_native_form(form)
    }
}

pub(super) struct InputNamespaceScope(Arc<Evaluation>);
impl Drop for InputNamespaceScope {
    fn drop(&mut self) {
        self.0.input_active.store(false, Ordering::SeqCst);
        let base = registry_lock(&self.0.input_base).take();
        drop(base);
    }
}

impl SessionHost<'_> {
    /// Retained before user code runs, independently of public root
    /// redefinitions and thread bindings (including helpers without AOT links).
    pub(crate) fn original_core_function(&mut self, name: &str) -> Result<SessionValue, SessionError> {
        let state = self.evaluation.as_ref().ok_or_else(|| wasmtime::Error::msg("Missing native compiler helper authority"))?;
        let function = registry_lock(&state.compiler_helpers).get(name).cloned()
            .ok_or_else(|| wasmtime::Error::msg(format!("Missing original compiled core helper {name}")))?;
        let mut roots = RootScope::new(&mut self.store);
        let value = Val::AnyRef(Some(function.to_rooted(&mut roots)));
        own(&mut roots, value, self.identity, &self.handles)
    }
    pub(crate) fn enter_compilation_namespace(&mut self, namespace: &SessionValue) -> Result<DynamicCheckpoint, SessionError> {
        self.check(namespace)?;
        self.charge_host_work(1)?;
        let cell = self.core_cell("*ns*")?;
        let mut scope = RootScope::new(&mut self.store);
        let checkpoint = dynamic_checkpoint(&mut scope, self.runtime)?;
        let result = (|| -> Result<(), SessionError> {
        let input = cell.get(&mut scope);
        let mut invoke = |name: &str, args: &[Val]| -> Result<SessionValue, SessionError> {
            let function = self.runtime.get_func(&mut scope, name).ok_or_else(|| wasmtime::Error::msg("Missing namespace frame primitive"))?;
            call(&mut scope, self.runtime, function, args, self.identity, &self.handles)
        };
        let old = invoke("binding-snapshot", &[input])?;
        let entries = invoke("args-new", &[Val::I32(3)])?;
        let array = entries.object()?.to_rooted(&mut scope).as_array(&scope)?.unwrap();
        let old = old.val(&mut scope.as_context_mut());
        let new = namespace.val(&mut scope.as_context_mut());
        for (index, value) in [input, old, new].into_iter().enumerate() { array.set(&mut scope, index as u32, value)?; }
        let entries = entries.val(&mut scope.as_context_mut());
        let push = self.runtime.get_func(&mut scope, "dynamic-push").unwrap();
        // call() restores its own checkpoint and would immediately undo this
        // frame. The caller owns this explicit checkpoint until leave below.
        let mut result = [Val::null_any_ref()];
        if let Err(error) = push.call(&mut scope, &[entries], &mut result) {
            return Err(execution_error(&mut scope, self.runtime, error, self.identity, &self.handles));
        }
        Ok(())
        })();
        if let Err(error) = result {
            let _ = restore_dynamic(&mut scope, self.runtime, &checkpoint);
            return Err(error);
        }
        Ok(checkpoint)
    }
    pub(crate) fn leave_compilation_namespace(&mut self, checkpoint: &DynamicCheckpoint) -> Result<(), SessionError> {
        let mut scope = RootScope::new(&mut self.store);
        restore_dynamic(&mut scope, self.runtime, checkpoint)
    }
    pub(crate) fn local_macro_authority(&self) -> Option<MacroAuthority> {
        let state = self.evaluation.as_ref()?;
        let authority = registry_lock(&state.local).clone()?;
        let service = registry_lock(&state.external).clone()?;
        Some(authority.with_fallback(service))
    }
    pub(crate) fn owned_macro_authority(&self) -> Option<MacroAuthority> {
        let state = self.evaluation.as_ref()?;
        registry_lock(&state.own).clone()
    }
    pub(super) fn evaluation_input_scope(
        &mut self,
        input: bool,
    ) -> Result<Option<InputNamespaceScope>, SessionError> {
        if !input || self.store.is_borrowed() {
            return Ok(None);
        }
        let Some(state) = self.evaluation.clone() else {
            return Ok(None);
        };
        if state.input_active.load(Ordering::SeqCst) {
            return Err(
                wasmtime::Error::msg("Native input namespace entry is already active").into(),
            );
        }
        let value = self
            .runtime
            .get_global(&mut self.store, "dynamic-frame")
            .ok_or_else(|| wasmtime::Error::msg("Missing dynamic frame checkpoint"))?
            .get(&mut self.store);
        let frame = value
            .unwrap_anyref()
            .map(|frame| frame.to_owned_rooted(&mut self.store))
            .transpose()?;
        *registry_lock(&state.input_base) = frame;
        state.input_active.store(true, Ordering::SeqCst);
        Ok(Some(InputNamespaceScope(state)))
    }
    pub(crate) fn evaluation_updates_input_namespace(&mut self) -> Result<bool, SessionError> {
        let state = self
            .evaluation
            .as_ref()
            .ok_or_else(|| wasmtime::Error::msg("Missing evaluation authority"))?;
        if !state.input_active.load(Ordering::SeqCst) {
            return Ok(false);
        }
        let namespace = registry_lock(&state.namespace)
            .clone()
            .ok_or_else(|| wasmtime::Error::msg("Missing namespace authority"))?;
        let base = registry_lock(&state.input_base).clone();
        let mut scope = RootScope::new(&mut self.store);
        let cell = namespace.cell.get(&mut scope);
        let base = Val::AnyRef(base.map(|frame| frame.to_rooted(&mut scope)));
        let function = self
            .runtime
            .get_func(&mut scope, "dynamic-namespace-input-owner")
            .ok_or_else(|| wasmtime::Error::msg("Missing input namespace owner classifier"))?;
        let mut result = [Val::I32(0)];
        function.call(&mut scope, &[cell, base], &mut result)?;
        Ok(result[0].unwrap_i32() != 0)
    }
    pub(crate) fn evaluation_namespace(&mut self) -> Result<String, SessionError> {
        let state = self
            .evaluation
            .as_ref()
            .ok_or_else(|| wasmtime::Error::msg("Missing native evaluation authority"))?;
        let namespace = registry_lock(&state.namespace)
            .clone()
            .ok_or_else(|| wasmtime::Error::msg("Missing native namespace authority"))?;
        namespace.read(self)
    }
    /// A snapshot is useful for resolution, but authorization follows the real
    /// Var at each use: an earlier transformer can recursively replace its root.
    pub(crate) fn native_macro_enabled(&mut self, global: &CellIdentity) -> Result<Option<bool>, SessionError> {
        self.charge_host_work((global.namespace().len() + global.name().len()) as u64 * 4 + 16)?;
        if global.phase() != self.phase { return Ok(None); }
        let cell = registry_lock(&self.compiler).environment.existing_var_cell(global);
        let Some(cell) = cell else { return Ok(None); };
        if !registry_lock(&self.compiler).cells.contains_key(&cell) { return Ok(None); }
        let variable = self.binding_value(&cell)?;
        if variable.value.is_none() { return Ok(None); }
        let state = self.evaluation.clone().ok_or_else(|| wasmtime::Error::msg("Missing evaluation authority"))?;
        let descriptor = registry_lock(&state.var_class).clone().ok_or_else(|| wasmtime::Error::msg("Missing Var authority"))?;
        let actual = self.data_descriptor(&variable, false)?;
        let matches = {
            let mut roots = RootScope::new(&mut self.store);
            let actual = actual.object()?.to_rooted(&mut roots);
            let expected = descriptor.to_rooted(&mut roots);
            wasmtime::Rooted::ref_eq(&roots, &actual, &expected)?
        };
        if !matches { return Err(wasmtime::Error::msg("Macro Var has a foreign descriptor").into()); }
        let fields = self.data_fields(&variable)?;
        if fields.len() != 3 { return Err(wasmtime::Error::msg("Macro Var has invalid fields").into()); }
        let root_is_macro = {
            let mut roots = RootScope::new(&mut self.store);
            let getter = fields[0].object()?.to_rooted(&mut roots);
            let check = self.runtime.get_func(&mut roots, "binding-getter-macro?")
                .ok_or_else(|| wasmtime::Error::msg("Missing Var macro flag reader"))?;
            let mut result = [Val::I32(0)];
            check.call(&mut roots, &[Val::AnyRef(Some(getter))], &mut result)?;
            result[0].unwrap_i32() != 0
        };
        if !root_is_macro { return Ok(Some(false)); }
        let cached = registry_lock(&state.macro_metadata).get(global).cloned();
        if let Some(cached) = cached {
            let same = match (&fields[2].value, &cached.metadata) {
                (None, None) => true,
                (Some(current), Some(previous)) => {
                    let mut roots = RootScope::new(&mut self.store);
                    let current = current.to_rooted(&mut roots);
                    let previous = previous.to_rooted(&mut roots);
                    wasmtime::Rooted::ref_eq(&roots, &current, &previous)?
                }
                _ => false,
            };
            if same { return Ok(Some(cached.enabled)); }
        }
        let bridge = state.bridge(self)?;
        let lookup = registry_lock(&state.metadata_lookup).clone()
            .ok_or_else(|| wasmtime::Error::msg("Missing native metadata lookup"))?;
        let lookup = {
            let mut roots = RootScope::new(&mut self.store);
            let lookup = Val::AnyRef(Some(lookup.to_rooted(&mut roots)));
            own(&mut roots, lookup, self.identity, &self.handles)?
        };
        let key = bridge.quote(self, suss_reader::forms::Form {
            span: 0..0, metadata: vec![], kind: suss_reader::forms::Kind::Keyword(suss_reader::Keyword::new("macro")),
        })?;
        let flag = self.invoke(&lookup, &[&fields[2], &key])?;
        let enabled = self.inspect(&flag, |mut store, value| {
            Ok(!matches!(sentinel_tag(&mut store, &value)?, Some(tag) if tag == runtime_abi::NIL as u32 || tag == runtime_abi::FALSE as u32))
        })?;
        registry_lock(&state.macro_metadata).insert(global.clone(), MacroMetadata {
            metadata: fields[2].value.clone(), enabled,
        });
        Ok(Some(enabled))
    }
    pub(crate) fn synchronize_evaluation_macros(&mut self, snapshot: &mut CompilationSnapshot) -> Result<(), SessionError> {
        let mut statuses = BTreeMap::new();
        for global in snapshot.environment.native_macro_declarations(self.identity) {
            if let Some(enabled) = self.native_macro_enabled(&global)? { statuses.insert(global, enabled); }
        }
        snapshot.environment.replace_native_macro_status(statuses);
        Ok(())
    }
    pub(crate) fn evaluation_aliases(&mut self) -> Result<BTreeMap<String, String>, SessionError> {
        let state = self
            .evaluation
            .clone()
            .ok_or_else(|| wasmtime::Error::msg("Missing native evaluation authority"))?;
        let namespace = registry_lock(&state.namespace)
            .clone()
            .ok_or_else(|| wasmtime::Error::msg("Missing native namespace authority"))?;
        let bridge = state.bridge(self)?;
        namespace.aliases(self, &bridge)
    }
    fn core_cell(&self, name: &str) -> Result<Global, SessionError> {
        let compiler = registry_lock(&self.compiler);
        let global = compiler
            .environment
            .resolve(
                self.phase,
                &suss_reader::Symbol::namespaced("suss.core", name),
                0..0,
            )
            .map_err(SessionError::Compile)?
            .global()
            .clone();
        compiler
            .cells
            .get(&global)
            .copied()
            .ok_or_else(|| wasmtime::Error::msg("Missing core binding cell").into())
    }
    pub(crate) fn attach_expander(&mut self, expander: &dyn portable::ExpansionHost) {
        let Some(service) = expander
            .persistent_macro_context()
            .and_then(|value| value.downcast_ref::<MacroService>())
        else {
            return;
        };
        if let Some(evaluation) = &self.evaluation {
            *registry_lock(&evaluation.external) = Some(service.clone());
        }
    }
    pub(crate) fn attach_macro_authority(&mut self, authority: MacroAuthority) {
        if let Some(evaluation) = &self.evaluation {
            *registry_lock(&evaluation.bridge) = Some(authority.bridge());
            *registry_lock(&evaluation.own) = Some(authority);
        }
    }
    pub(super) fn refresh_evaluation_profile(&self) {
        if let Some(evaluation) = &self.evaluation {
            *registry_lock(&evaluation.profile) = Some(Arc::new(self.reentry_profile()));
        }
    }
    pub(super) fn install_evaluation(&mut self) -> Result<(), SessionError> {
        if self.evaluation.is_some() {
            return Ok(());
        }
        let state = Arc::new(Evaluation::default());
        for name in ["intern-namespace", "enter-macro-namespace!", "ex-message"] {
            let cell = self.core_cell(name)?;
            let mut roots = RootScope::new(&mut self.store);
            let get = self.runtime.get_func(&mut roots, "binding-get").unwrap();
            let cell = cell.get(&mut roots);
            let value = call(&mut roots, self.runtime, get, &[cell], self.identity, &self.handles)?;
            registry_lock(&state.compiler_helpers).insert(name, value.object()?.clone());
        }
        *registry_lock(&state.namespace) = Some(NamespaceAccess::capture(self)?);
        let cell = self.core_cell("Var")?;
        let value = {
            let mut roots = RootScope::new(&mut self.store);
            let get = self.runtime.get_func(&mut roots, "binding-get").unwrap();
            let cell = cell.get(&mut roots);
            call(
                &mut roots,
                self.runtime,
                get,
                &[cell],
                self.identity,
                &self.handles,
            )?
        };
        let descriptor = self.data_descriptor(&value, true)?;
        *registry_lock(&state.var_class) = Some(descriptor.object()?.clone());
        registry_lock(&state.compiler_helpers).insert("Var", value.object()?.clone());
        let namespace_constructor = {
            let cell = self.core_cell("Namespace")?;
            let mut roots = RootScope::new(&mut self.store);
            let get = self.runtime.get_func(&mut roots, "binding-get").unwrap();
            let cell = cell.get(&mut roots);
            call(&mut roots, self.runtime, get, &[cell], self.identity, &self.handles)?
        };
        registry_lock(&state.compiler_helpers).insert("Namespace", namespace_constructor.object()?.clone());
        let cell = self.core_cell("MetaFn")?;
        let value = {
            let mut roots = RootScope::new(&mut self.store);
            let get = self.runtime.get_func(&mut roots, "binding-get").unwrap();
            let cell = cell.get(&mut roots);
            call(&mut roots, self.runtime, get, &[cell], self.identity, &self.handles)?
        };
        let descriptor = self.data_descriptor(&value, true)?;
        *registry_lock(&state.meta_fn_class) = Some(descriptor.object()?.clone());
        registry_lock(&state.compiler_helpers).insert("MetaFn", value.object()?.clone());

        // Retain core's original get function. The generic native invocation
        // ABI takes callable functions; keyword application is compiler lowering.
        let cell = self.core_cell("get")?;
        let value = {
            let mut roots = RootScope::new(&mut self.store);
            let get = self.runtime.get_func(&mut roots, "binding-get").unwrap();
            let cell = cell.get(&mut roots);
            call(
                &mut roots,
                self.runtime,
                get,
                &[cell],
                self.identity,
                &self.handles,
            )?
        };
        *registry_lock(&state.metadata_lookup) = Some(value.object()?.clone());
        let mut syntax_tests = Vec::new();
        for name in ["coll?", "symbol?"] {
            let cell = self.core_cell(name)?;
            let mut roots = RootScope::new(&mut self.store);
            let get = self.runtime.get_func(&mut roots, "binding-get").unwrap();
            let cell = cell.get(&mut roots);
            let test = call(&mut roots, self.runtime, get, &[cell], self.identity, &self.handles)?;
            syntax_tests.push(test.object()?.clone());
        }
        *registry_lock(&state.syntax_tests) = syntax_tests;
        // Capture the original constructor before exposing this Session to user
        // source; restoring an exception must not consult a rebound ex-info.
        let cell = self.core_cell("ex-info")?;
        let value = {
            let mut roots = RootScope::new(&mut self.store);
            let get = self.runtime.get_func(&mut roots, "binding-get").unwrap();
            let cell = cell.get(&mut roots);
            call(&mut roots, self.runtime, get, &[cell], self.identity, &self.handles)?
        };
        *registry_lock(&state.exception_info) = Some(value.object()?.clone());

        let runtime = self.runtime;
        let signature = runtime
            .get_func(&mut self.store, "evaluate-form")
            .ok_or_else(|| wasmtime::Error::msg("Missing evaluation capability"))?
            .ty(&self.store);
        let callback_state = state.clone();
        let callback = Func::try_new(
            &mut self.store,
            signature,
            move |mut caller, args, results| {
                let profile = registry_lock(&callback_state.profile)
                    .clone()
                    .ok_or_else(|| wasmtime::Error::msg("No active native compiler profile"))?;
                let result =
                    callback_state.evaluate(&profile, caller.as_context_mut(), args[0].clone());
                match result {
                    Ok(value) => {
                        results[0] = value.val(&mut caller.as_context_mut());
                        Ok(())
                    }
                    Err(SessionError::Trap(error)) => Err(error),
                    Err(error) => {
                        let mut view = profile.bind(caller.as_context_mut());
                        view.throw_evaluation_error(error)
                    }
                }
            },
        )?;
        runtime
            .get_func(&mut self.store, "evaluation-source-install")
            .ok_or_else(|| wasmtime::Error::msg("Missing evaluation installer"))?
            .call(&mut self.store, &[Val::FuncRef(Some(callback))], &mut [])?;
        self.evaluation = Some(state);
        self.refresh_evaluation_profile();
        self.install_native_identity(&self.evaluation.as_ref().unwrap().clone())?;
        self.install_native_operations(&self.evaluation.as_ref().unwrap().clone())?;
        Ok(())
    }
    fn throw_evaluation_error<T>(&mut self, error: SessionError) -> wasmtime::Result<T> {
        let payload = match error {
            SessionError::Language(value) => value,
            SessionError::Host(error) if error.is::<crate::portable_macro_data::TransferredException>() => {
                let state = self.evaluation.clone()
                    .ok_or_else(|| wasmtime::Error::msg("Missing exception restoration authority"))?;
                let bridge = state.bridge(self).map_err(native_error)?;
                let constructor = registry_lock(&state.exception_info).clone()
                    .ok_or_else(|| wasmtime::Error::msg("Missing original exception constructor"))?;
                let factory = {
                    let mut roots = RootScope::new(&mut self.store);
                    let value = Val::AnyRef(Some(constructor.to_rooted(&mut roots)));
                    own(&mut roots, value, self.identity, &self.handles).map_err(native_error)?
                };
                error.downcast_ref::<crate::portable_macro_data::TransferredException>().unwrap()
                    .restore(&bridge, self, &factory).map_err(native_error)?
            }
            error => {
                let text = error.to_string();
                let units = text.encode_utf16().collect::<Vec<_>>();
                self.charge_host_work(units.len() as u64 * 2 + 1)
                    .map_err(native_error)?;
                let runtime = self.runtime;
                let mut roots = RootScope::new(&mut self.store);
                let new = runtime.get_func(&mut roots, "string-new").unwrap();
                let string = call(
                    &mut roots,
                    runtime,
                    new,
                    &[Val::I32(i32::try_from(units.len())?)],
                    self.identity,
                    &self.handles,
                )
                .map_err(native_error)?;
                let array = string
                    .object()?
                    .to_rooted(&mut roots)
                    .as_array(&roots)?
                    .unwrap();
                for (index, unit) in units.into_iter().enumerate() {
                    array.set(&mut roots, index as u32, Val::I32(i32::from(unit)))?;
                }
                let new = runtime.get_func(&mut roots, "language-error-new").unwrap();
                let string = string.val(&mut roots.as_context_mut());
                call(
                    &mut roots,
                    runtime,
                    new,
                    &[string],
                    self.identity,
                    &self.handles,
                )
                .map_err(native_error)?
            }
        };
        let tag = self
            .runtime
            .get_tag(&mut self.store, "language-exception")
            .ok_or_else(|| wasmtime::Error::msg("Missing language exception tag"))?;
        let ty = wasmtime::ExnType::new(&self.engine, tag.ty(&self.store).ty().params())?;
        let allocator = wasmtime::ExnRefPre::new(&mut self.store, ty);
        let value = payload.val(&mut self.store.as_context_mut());
        let exception = wasmtime::ExnRef::new(&mut self.store, &allocator, &tag, &[value])?;
        self.store.as_context_mut().throw(exception)
    }
}

fn native_error(error: SessionError) -> wasmtime::Error {
    match error {
        SessionError::Trap(error) | SessionError::Host(error) => error,
        error => wasmtime::Error::msg(error.to_string()),
    }
}

impl SessionHost<'_> {
    pub(crate) fn native_owner(&self) -> u64 {
        self.identity
    }
    pub(crate) fn read_native_evaluation(
        &mut self,
        bridge: &FormBridge,
        value: &SessionValue,
        span: std::ops::Range<usize>,
    ) -> Result<suss_reader::forms::Form, SessionError> {
        let closure_test = self
            .runtime
            .get_func(&mut self.store, "closure-value-test")
            .ok_or_else(|| wasmtime::Error::msg("Missing closure value classifier"))?;
        let class_test = self
            .runtime
            .get_func(&mut self.store, "class-value-is")
            .ok_or_else(|| wasmtime::Error::msg("Missing class value classifier"))?;
        let instance_test = self
            .runtime
            .get_func(&mut self.store, "object-instance")
            .ok_or_else(|| wasmtime::Error::msg("Missing nominal instance classifier"))?;
        let identity_test = self.runtime.get_func(&mut self.store, "native-identical-test")
            .ok_or_else(|| wasmtime::Error::msg("Missing native identity test"))?;
        let evaluation = self
            .evaluation
            .as_ref()
            .ok_or_else(|| wasmtime::Error::msg("Missing native evaluation authority"))?;
        let var_class = registry_lock(&evaluation.var_class)
            .clone()
            .ok_or_else(|| wasmtime::Error::msg("Missing native Var authority"))?;
        let meta_fn_class = registry_lock(&evaluation.meta_fn_class).clone()
            .ok_or_else(|| wasmtime::Error::msg("Missing native metadata-function authority"))?;
        bridge.read_evaluation(
            self,
            value,
            span,
            self.identity,
            closure_test,
            class_test,
            instance_test,
            identity_test,
            var_class,
            meta_fn_class,
        )
    }
    pub(crate) fn retain_native_constant(
        &mut self,
        index: u64,
        value: OwnedRooted<AnyRef>,
        materialize_function: bool,
    ) -> Result<(), SessionError> {
        self.charge_host_work(1)?;
        let identity = CellIdentity::native_constant(self.phase, self.identity, index);
        let global = {
            let mut roots = RootScope::new(&mut self.store);
            let value = Val::AnyRef(Some(value.to_rooted(&mut roots)));
            let mut result = [Val::null_any_ref()];
            self.runtime
                .get_func(&mut roots, "binding-new")
                .unwrap()
                .call(&mut roots, &[value], &mut result)?;
            let ty = result[0]
                .unwrap_anyref()
                .unwrap()
                .as_struct(&roots)?
                .unwrap()
                .ty(&roots)?;
            Global::new(
                &mut roots,
                GlobalType::new(
                    ValType::Ref(RefType::new(false, ty.into())),
                    Mutability::Const,
                ),
                result[0],
            )?
        };
        // No compiler guard is held across a guest call or instantiation.
        let mut state = registry_lock(&self.compiler);
        state.linker.define(
            &self.store,
            identity.import_module(),
            &identity.import_name(),
            global,
        )?;
        if materialize_function {
            Arc::make_mut(&mut state.native_function_cells).insert(identity.import_name(), global);
        }
        let literal=CellIdentity::native_value(self.phase,self.identity,index);
        state.linker.define(&self.store,literal.import_module(),&literal.import_name(),global)?;
        Arc::make_mut(&mut state.cells).insert(literal,global);
        Arc::make_mut(&mut state.cells).insert(identity, global);
        Ok(())
    }
    pub(crate) fn native_constant_value(
        &mut self,
        owner: u64,
        index: u64,
    ) -> Result<SessionValue, SessionError> {
        if owner != self.identity {
            return Err(
                wasmtime::Error::msg("Native constant cannot cross phase Store ownership").into(),
            );
        }
        self.binding_value(&CellIdentity::native_constant(self.phase, owner, index))
    }
}
