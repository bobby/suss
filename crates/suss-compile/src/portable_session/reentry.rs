//! A request-scoped native authority snapshot, excluding the Store itself.
//! Capture immediately before entering a Store; configuration is fixed for that
//! entry. Compiler publication, requests, recovery and budget depth remain shared.
use super::*;

pub(crate) struct ReentryProfile {
    engine: Engine,
    phase: Phase,
    options: SessionOptions,
    identity: u64,
    handles: Arc<AtomicUsize>,
    interrupt: Arc<AtomicBool>,
    operation_budget_depth: Arc<AtomicUsize>,
    runtime: Instance,
    numeric_memory: Memory,
    compiler: SharedCompiler,
    base_runtime_artifact_bytes: usize,
    bootstrap_core: bool,
    pending_host_futures: SharedPendingRegistry,
    native_factories: Arc<Mutex<Vec<native_async::FactoryProfile>>>,
    last_async_recovered_owner: Arc<Mutex<Option<OwnedRooted<AnyRef>>>>,
}
impl SessionHost<'_> {
    /// Does not borrow the Store or retain a Session pointer. The resulting
    /// profile may bind only the same Store's active Caller, within this entry.
    pub(crate) fn reentry_profile(&self) -> ReentryProfile {
        ReentryProfile {
            engine: self.engine.clone(),
            phase: self.phase,
            options: self.options.clone(),
            identity: self.identity,
            handles: self.handles.clone(),
            interrupt: self.interrupt.clone(),
            operation_budget_depth: self.operation_budget_depth.clone(),
            runtime: self.runtime,
            numeric_memory: self.numeric_memory,
            compiler: self.compiler.clone(),
            base_runtime_artifact_bytes: self.base_runtime_artifact_bytes,
            bootstrap_core: self.bootstrap_core,
            pending_host_futures: self.pending_host_futures.clone(),
            native_factories: self.native_factories.clone(),
            last_async_recovered_owner: self.last_async_recovered_owner.clone(),
        }
    }
}
impl ReentryProfile {
    /// The caller owns the unique Store borrow for the lifetime of this view.
    /// Form transport/invocation reuse the owned-session implementation.
    pub(crate) fn bind<'a>(&self, store: StoreContextMut<'a, ()>) -> SessionHost<'a> {
        SessionHost {
            engine: self.engine.clone(),
            phase: self.phase,
            options: self.options.clone(),
            identity: self.identity,
            handles: self.handles.clone(),
            interrupt: self.interrupt.clone(),
            operation_budget_depth: self.operation_budget_depth.clone(),
            store: store_access::StoreAccess::Borrowed(store),
            runtime: self.runtime,
            numeric_memory: self.numeric_memory,
            compiler: self.compiler.clone(),
            base_runtime_artifact_bytes: self.base_runtime_artifact_bytes,
            bootstrap_core: self.bootstrap_core,
            pending_host_futures: self.pending_host_futures.clone(),
            native_factories: self.native_factories.clone(),
            last_async_recovered_owner: self.last_async_recovered_owner.clone(),
        }
    }
}

#[cfg(all(test, not(suss_generator)))]
mod tests {
    use super::*;
    use crate::portable_macro_data::FormBridge;

    #[test]
    fn borrowed_session_reuses_data_and_invocation_without_refilling_or_retiring_owner() {
        fn assert_send<T: Send>() {}
        assert_send::<Session>();
        for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
            let bridge = Arc::new(FormBridge::new(&mut session).unwrap());
            let transformer = session.eval(
                "(fn* [n env] (if (= (:context env) :expr) (list '+ n 1/2) (throw (ex-info \"missing expansion context\" {}))))"
            ).unwrap();
            let cancellations = Arc::new(AtomicUsize::new(0));
            let cancelled = cancellations.clone();
            let pending = session
                .pending_future_with_cancel(move || {
                    cancelled.fetch_add(1, Ordering::SeqCst);
                })
                .unwrap();
            let input = session.eval("1N").unwrap();
            // This profile belongs to the immediately following Store entry.
            let profile = session.reentry_profile();
            let expected_depth = session.operation_budget_depth.load(Ordering::SeqCst);
            let runtime = session.runtime;
            let signature = runtime
                .get_func(&mut session.store, "evaluate-form")
                .unwrap()
                .ty(&session.store);
            let data = bridge.clone();
            let callback = Func::try_new(
                &mut session.store,
                signature,
                move |mut caller, args, results| {
                    let mut view = profile.bind(caller.as_context_mut());
                    let before = view.store.get_fuel()?;
                    let input = {
                        let mut roots = RootScope::new(&mut view.store);
                        own(&mut roots, args[0].clone(), view.identity, &view.handles)
                            .map_err(|error| wasmtime::Error::msg(error.to_string()))?
                    };
                    // The ordinary analysis graph must also work through the
                    // active Caller, including its original construction meter.
                    let snapshot = view.compilation_snapshot();
                    let namespace = Arc::new(crate::portable::hir::SourceNamespace::capture(
                        &snapshot.environment, view.phase,
                    ));
                    let environment = crate::portable_macro_graph::AnalysisGraph::new(
                        &data, &mut view,
                    ).expansion(crate::portable::ExpansionContext {
                        environment: &snapshot.environment,
                        namespace_snapshot: &namespace,
                        origin: None,
                        phase: snapshot.phase,
                        context: crate::portable::AnalysisContext::Expression,
                        locals: &std::collections::HashMap::new(),
                        fields: &std::collections::HashMap::new(),
                        function_scopes: &[],
                    }).map_err(|error| wasmtime::Error::msg(error.to_string()))?;
                    let value = view
                        .invoke(&transformer, &[&input, &environment])
                        .map_err(|error| wasmtime::Error::msg(error.to_string()))?;
                    let form = data
                        .read(&mut view, &value, 0..0)
                        .map_err(|error| wasmtime::Error::msg(error.to_string()))?;
                    let quoted = data
                        .quote(&mut view, form)
                        .map_err(|error| wasmtime::Error::msg(error.to_string()))?;
                    let form = data
                        .read(&mut view, &quoted, 0..0)
                        .map_err(|error| wasmtime::Error::msg(error.to_string()))?;
                    let value = view
                        .eval_forms(vec![form], 0..0)
                        .map_err(|error| wasmtime::Error::msg(error.to_string()))?;
                    let after = view.store.get_fuel()?;
                    assert!(
                        after < before && before <= 1_000_000,
                        "borrowed operations must debit the caller's existing budget"
                    );
                    assert_eq!(
                        view.operation_budget_depth.load(Ordering::SeqCst),
                        expected_depth
                    );
                    assert!(
                        view.reset().is_err(),
                        "a borrowed view cannot replace its owner"
                    );
                    results[0] = value.val(&mut view.store.as_context_mut());
                    Ok(())
                },
            )
            .unwrap();
            runtime
                .get_func(&mut session.store, "evaluation-source-install")
                .unwrap()
                .call(&mut session.store, &[Val::FuncRef(Some(callback))], &mut [])
                .unwrap();
            let module = wasmtime::Module::new(
                &session.engine,
                r#"(module
                (import "runtime" "evaluate-form" (func $evaluate (param eqref) (result eqref)))
                (func (export "go") (param eqref) (result eqref) local.get 0 call $evaluate))"#,
            )
            .unwrap();
            let mut linker = Linker::new(&session.engine);
            linker
                .instance(&mut session.store, "runtime", runtime)
                .unwrap();
            let instance = linker.instantiate(&mut session.store, &module).unwrap();
            let go = instance.get_func(&mut session.store, "go").unwrap();
            session.store.set_fuel(1_000_000).unwrap();
            let value = {
                let mut roots = RootScope::new(&mut session.store);
                let input = input.val(&mut roots.as_context_mut());
                call(
                    &mut roots,
                    runtime,
                    go,
                    &[input],
                    session.identity,
                    &session.handles,
                )
                .unwrap()
            };
            assert_eq!(
                cancellations.load(Ordering::SeqCst),
                0,
                "dropping a view must not retire owner requests"
            );
            assert_eq!(session.stats().pending_host_requests, 1);
            session.collect().unwrap();
            let observed = bridge.read(&mut session, &value, 0..0).unwrap();
            let expected = suss_reader::forms::read_forms("3/2").unwrap().remove(0);
            assert_eq!(observed.kind, expected.kind);
            // Cancellation follows the existing host contract: consume the
            // request and exhaust fuel, rather than preserve a sticky flag.
            let profile = session.reentry_profile();
            let signature = runtime
                .get_func(&mut session.store, "evaluate-form")
                .unwrap()
                .ty(&session.store);
            let cancelled_entry =
                Func::try_new(&mut session.store, signature, move |mut caller, _, _| {
                    let mut view = profile.bind(caller.as_context_mut());
                    view.interrupt.store(true, Ordering::SeqCst);
                    let interrupted = view
                        .data_scalar(&portable::hir::Literal::Long(7))
                        .unwrap_err();
                    assert!(interrupted.is_interrupt());
                    assert!(!view.interrupt.load(Ordering::SeqCst));
                    assert_eq!(view.store.get_fuel()?, 0);
                    let retry = view
                        .data_scalar(&portable::hir::Literal::Long(7))
                        .unwrap_err();
                    assert!(matches!(retry, SessionError::Trap(ref error)
                    if error.downcast_ref::<wasmtime::Trap>()==Some(&wasmtime::Trap::OutOfFuel)));
                    assert_eq!(
                        view.store.get_fuel()?,
                        0,
                        "retry must not refill an exhausted entry"
                    );
                    Err(wasmtime::Trap::Interrupt.into())
                })
                .unwrap();
            runtime
                .get_func(&mut session.store, "evaluation-source-install")
                .unwrap()
                .call(
                    &mut session.store,
                    &[Val::FuncRef(Some(cancelled_entry))],
                    &mut [],
                )
                .unwrap();
            session.store.set_fuel(10_000).unwrap();
            let failure = {
                let mut roots = RootScope::new(&mut session.store);
                call(
                    &mut roots,
                    runtime,
                    go,
                    &[Val::null_any_ref()],
                    session.identity,
                    &session.handles,
                )
                .unwrap_err()
            };
            assert!(failure.is_interrupt());
            assert_eq!(session.store.get_fuel().unwrap(), 0);
            assert_eq!(cancellations.load(Ordering::SeqCst), 0);
            assert_eq!(session.stats().pending_host_requests, 1);
            drop(pending);
            drop(session);
            assert_eq!(
                cancellations.load(Ordering::SeqCst),
                1,
                "only owner teardown cancels its request"
            );
        }
    }
}
