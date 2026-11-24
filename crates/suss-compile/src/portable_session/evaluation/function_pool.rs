//! Private per-source-function constant pools. Compiler-emitted calls initialize
//! the pool when its source function is constructed, before any of its body runs.
//! Callbacks capture authenticated cells in this Store; no caller-supplied cell,
//! serialized function representation, or foreign GC reference is accepted.
use super::*;

pub(in crate::portable_session) fn initializer(
    mut store: StoreContextMut<'_, ()>,
    runtime: Instance,
    evaluation: Option<&Arc<Evaluation>>,
    source: Global,
    target: Global,
    function: bool,
) -> Result<Global, SessionError> {
    let state = evaluation
        .map(Arc::downgrade)
        .ok_or_else(|| wasmtime::Error::msg("Missing native function-pool authority"))?;
    let constructor = runtime.get_func(&mut store, "closure-new").unwrap();
    let parameter = constructor.ty(&store).params().nth(1).unwrap();
    let wasmtime::HeapType::ConcreteFunc(signature) = parameter.unwrap_ref().heap_type() else {
        return Err(wasmtime::Error::msg("Invalid native closure invocation type").into());
    };
    let initialized = AtomicBool::new(false);
    let callback = Func::try_new(
        &mut store,
        signature.clone(),
        move |mut caller, _arguments, results| {
            let state = state
                .upgrade()
                .ok_or_else(|| wasmtime::Error::msg("Native compiler owner has retired"))?;
            let profile = registry_lock(&state.profile)
                .clone()
                .ok_or_else(|| wasmtime::Error::msg("No active native compiler profile"))?;
            let mut view = profile.bind(caller.as_context_mut());
            let result = if initialized.load(Ordering::Relaxed) {
                view.charge_host_work(1)
            } else {
                view.initialize_function_constant(source, target, function)
            };
            match result {
                Ok(()) => {
                    initialized.store(true, Ordering::Relaxed);
                    results[0] = Val::null_any_ref();
                    Ok(())
                }
                Err(SessionError::Trap(error)) => Err(error),
                Err(error) => view.throw_evaluation_error(error),
            }
        },
    )?;
    let mut roots = RootScope::new(&mut store);
    let mut result = [Val::null_any_ref()];
    constructor.call(
        &mut roots,
        &[
            Val::null_any_ref(),
            Val::FuncRef(Some(callback)),
            Val::I32(0),
            Val::I32(0),
        ],
        &mut result,
    )?;
    runtime.get_func(&mut roots, "binding-new").unwrap().call(
        &mut roots,
        &[result[0]],
        &mut result,
    )?;
    let ty = result[0]
        .unwrap_anyref()
        .unwrap()
        .as_struct(&roots)?
        .unwrap()
        .ty(&roots)?;
    Ok(Global::new(
        &mut roots,
        GlobalType::new(
            ValType::Ref(RefType::new(false, ty.into())),
            Mutability::Const,
        ),
        result[0],
    )?)
}

impl SessionHost<'_> {
    fn initialize_function_constant(
        &mut self,
        source: Global,
        target: Global,
        function: bool,
    ) -> Result<(), SessionError> {
        self.charge_host_work(1)?;
        let runtime = self.runtime;
        let mut scope = RootScope::new(&mut self.store);
        let mut result = [Val::null_any_ref()];
        let source = source.get(&mut scope);
        runtime.get_func(&mut scope, "binding-get").unwrap().call(
            &mut scope,
            &[source],
            &mut result,
        )?;
        if function {
            let mut classified = [Val::I32(0)];
            runtime
                .get_func(&mut scope, "closure-value-test")
                .unwrap()
                .call(&mut scope, &result, &mut classified)?;
            if classified[0].unwrap_i32() == 0 {
                return Err(wasmtime::Error::msg(
                    "Cannot initialize a metadata-wrapped function constant",
                )
                .into());
            }
            let object = result[0]
                .unwrap_anyref()
                .ok_or_else(|| wasmtime::Error::msg("Missing function constant"))?
                .as_struct(&scope)?
                .ok_or_else(|| wasmtime::Error::msg("Invalid function constant layout"))?;
            let ty = object.ty(&scope)?;
            let mut fields = object.fields(&mut scope)?.collect::<Vec<_>>();
            // Universal capture-free factories have runtime invocation/arity
            // bookkeeping and a nil environment, rather than lexical captures.
            let universal = fields.len() == runtime_abi::UNIVERSAL_MAX as usize + 1
                && matches!(
                    fields[runtime_abi::UNIVERSAL_ENVIRONMENT as usize],
                    Val::AnyRef(None)
                )
                && matches!(
                    fields[runtime_abi::UNIVERSAL_INVOKE as usize],
                    Val::FuncRef(Some(_))
                )
                && matches!(fields[runtime_abi::UNIVERSAL_MIN as usize], Val::I32(_))
                && matches!(fields[runtime_abi::UNIVERSAL_MAX as usize], Val::I32(_));
            if fields.len() != runtime_abi::CLOSURE_FIELDS as usize && !universal {
                return Err(wasmtime::Error::msg(
                    "Cannot initialize a function constant with captured values",
                )
                .into());
            }
            fields[runtime_abi::CLOSURE_PROPERTIES as usize] = Val::null_any_ref();
            fields[runtime_abi::CLOSURE_UID as usize] = Val::null_any_ref();
            let allocator = wasmtime::StructRefPre::new(&mut scope, ty);
            let fresh = wasmtime::StructRef::new(&mut scope, &allocator, &fields)?;
            result[0] = Val::AnyRef(Some(fresh.to_anyref()));
        }
        let target = target.get(&mut scope);
        runtime.get_func(&mut scope, "binding-set").unwrap().call(
            &mut scope,
            &[target, result[0]],
            &mut [],
        )?;
        Ok(())
    }
}
