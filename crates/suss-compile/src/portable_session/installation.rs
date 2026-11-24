//! Shared artifact validation, binding staging and instantiation for owned and
//! borrowed Store contexts. Publication remains the caller's explicit step.
use super::*;

pub(super) struct Installation {
    pub linker: Linker<()>,
    pub cells: BTreeMap<CellIdentity, Global>,
    pub instances: Vec<Instance>,
    pub artifact_bytes: usize,
}

pub(super) fn install(
    mut store: StoreContextMut<'_, ()>,
    engine: &Engine,
    runtime: Instance,
    phase: Phase,
    linker: &Linker<()>,
    known_cells: &BTreeMap<CellIdentity, Global>,
    bytes: &[&[u8]],
    cells: &[CellIdentity],
    interrupt: &AtomicBool,
) -> Result<Installation, SessionError> {
    Session::consume_host_work(store.as_context_mut(), 0, interrupt)?;
    let mut compiled = Vec::new();
    for bytes in bytes {
        runtime_abi::verify_artifact(bytes, &runtime_abi::Manifest::default())
            .map_err(|message| SessionError::Host(wasmtime::Error::msg(message)))?;
        portable::artifact_identity::verify(
            bytes,
            portable::artifact_identity::Expected {
                phase: Some(phase),
                ..Default::default()
            },
        )
        .map_err(|message| SessionError::Host(wasmtime::Error::msg(message)))?;
        let module = crate::portable_module_cache::compile(engine, bytes)?;
        Session::consume_host_work(store.as_context_mut(), 0, interrupt)?;
        compiled.push(module);
    }
    let mut linker = linker.clone();
    let mut staged = BTreeMap::new();
    for identity in cells {
        if known_cells.contains_key(identity) || staged.contains_key(identity) {
            continue;
        }
        let mut scope = RootScope::new(&mut store);
        let mut result = [Val::null_any_ref()];
        runtime
            .get_func(&mut scope, "binding-unbound")
            .unwrap()
            .call(&mut scope, &[], &mut result)?;
        let ty = result[0]
            .unwrap_anyref()
            .unwrap()
            .as_struct(&scope)?
            .unwrap()
            .ty(&scope)?;
        let global = Global::new(
            &mut scope,
            GlobalType::new(
                ValType::Ref(RefType::new(false, ty.into())),
                Mutability::Const,
            ),
            result[0].clone(),
        )?;
        linker.define(
            &scope,
            identity.import_module(),
            &identity.import_name(),
            global,
        )?;
        staged.insert(identity.clone(), global);
    }
    let mut instances = Vec::new();
    for module in compiled {
        instances.push(linker.instantiate(&mut store, &module)?);
    }
    Ok(Installation {
        linker,
        cells: staged,
        instances,
        artifact_bytes: bytes.iter().map(|bytes| bytes.len()).sum(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasmtime::Caller;

    #[test]
    fn borrowed_store_installation_compiles_inside_a_wasm_callback() {
        for phase in [Phase::Runtime, Phase::Macro] {
            let mut session = Session::with_options_in(SessionOptions::default(), phase).unwrap();
            session.eval("(def counter 0)").unwrap();
            let engine = session.engine.clone();
            let runtime = session.runtime;
            let environment = session.environment.clone();
            let cells = session.cells.clone();
            let linker = session.linker.clone();
            let interrupt = session.interrupt.clone();
            let evaluation = runtime
                .get_func(&mut session.store, "evaluate-form")
                .unwrap();
            let signature = evaluation.ty(&session.store);
            // A transport fixture compiles a real fragment against existing
            // cells while the same Store is executing its caller. Public eval
            // data decoding, macros and publication have separate regressions.
            let callback = Func::try_new(
                &mut session.store,
                signature,
                move |mut caller: Caller<'_, ()>, _args, results| {
                    let fragment = portable::prepare_fragment(
                        "(do (set! counter (+ counter 1)) (+ counter 40))",
                        &environment,
                        phase,
                    )
                    .map_err(|error| wasmtime::Error::msg(error.to_string()))?;
                    let installed = install(
                        caller.as_context_mut(),
                        &engine,
                        runtime,
                        phase,
                        &linker,
                        &cells,
                        &[&fragment.wasm],
                        &fragment.cells,
                        &interrupt,
                    )
                    .map_err(|error| wasmtime::Error::msg(error.to_string()))?;
                    let instance = installed.instances[0];
                    let eval = instance
                        .get_func(&mut caller, "eval")
                        .ok_or_else(|| wasmtime::Error::msg("Missing nested fragment eval"))?;
                    eval.call(&mut caller, &[], results)?;
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
                (func (export "go") (result eqref) ref.null eq call $evaluate))"#,
            )
            .unwrap();
            let mut caller_linker = Linker::new(&session.engine);
            caller_linker
                .instance(&mut session.store, "runtime", runtime)
                .unwrap();
            let caller = caller_linker
                .instantiate(&mut session.store, &module)
                .unwrap();
            let go = caller.get_func(&mut session.store, "go").unwrap();
            let mut result = [Val::null_any_ref()];
            for wanted in [41, 42] {
                session
                    .store
                    .set_fuel(portable::DEFAULT_OPERATION_FUEL)
                    .unwrap();
                go.call(&mut session.store, &[], &mut result).unwrap();
                assert!(
                    matches!(runtime_number(&mut session.store, &result[0]).unwrap(),
                    Some(RuntimeNumber::Long(value)) if value==wanted)
                );
                session.store.gc(None).unwrap();
            }
            let value = session.eval("counter").unwrap();
            assert!(matches!(
                session
                    .inspect(&value, |mut store, value| runtime_number(
                        &mut store, &value
                    ))
                    .unwrap(),
                Some(RuntimeNumber::Long(2))
            ));
        }
    }

    #[test]
    fn borrowed_store_compiler_consumes_actual_nominal_form_data() {
        for mut session in [Session::new_repl().unwrap(), Session::new_macro().unwrap()] {
            let bridge = crate::portable_macro_data::FormBridge::new(&mut session).unwrap();
            session.eval("(def counter 0)").unwrap();
            let reader = bridge.borrowed_reader(&mut session).unwrap();
            let engine = session.engine.clone();
            let runtime = session.runtime;
            let phase = session.phase;
            let environment = session.environment.clone();
            let cells = session.cells.clone();
            let linker = session.linker.clone();
            let interrupt = session.interrupt.clone();
            let evaluation = runtime
                .get_func(&mut session.store, "evaluate-form")
                .unwrap();
            let signature = evaluation.ty(&session.store);
            let callback = Func::try_new(
                &mut session.store,
                signature,
                move |mut caller, args, results| {
                    let form = reader.read(caller.as_context_mut(), &args[0], 0..0)?;
                    let fragment =
                        portable::prepare_fragment_forms(vec![form], 0..0, &environment, phase)
                            .map_err(|error| wasmtime::Error::msg(error.to_string()))?;
                    let installed = install(
                        caller.as_context_mut(),
                        &engine,
                        runtime,
                        phase,
                        &linker,
                        &cells,
                        &[&fragment.wasm],
                        &fragment.cells,
                        &interrupt,
                    )
                    .map_err(|error| wasmtime::Error::msg(error.to_string()))?;
                    installed.instances[0]
                        .get_func(&mut caller, "eval")
                        .unwrap()
                        .call(&mut caller, &[], results)
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
            let mut caller_linker = Linker::new(&session.engine);
            caller_linker
                .instance(&mut session.store, "runtime", runtime)
                .unwrap();
            let instance = caller_linker
                .instantiate(&mut session.store, &module)
                .unwrap();
            let go = instance.get_func(&mut session.store, "go").unwrap();
            for (source, expected) in [
                ("'(do (set! counter (+ counter 1)) (/ counter 6))", "1/6"),
                ("(list '* 2N 3N)", "6N"),
                ("'(let [x 1/2] (+ x 1/3))", "5/6"),
            ] {
                let input = session.eval(source).unwrap();
                session.budget().unwrap();
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
                session.collect().unwrap();
                let observed = bridge.read(&mut session, &value, 0..0).unwrap();
                let expected = suss_reader::forms::read_forms(expected).unwrap().remove(0);
                assert_eq!(observed.kind, expected.kind, "{source}");
            }
            let counter = session.eval("counter").unwrap();
            assert!(matches!(
                session
                    .inspect(&counter, |mut store, value| runtime_number(
                        &mut store, &value
                    ))
                    .unwrap(),
                Some(RuntimeNumber::Long(1))
            ));
        }
    }
}
