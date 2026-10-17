//! Actual Macro protocol dispatch scope. Observation only: no memo receipts,
//! setter capability, or source-data authentication is granted by these frames.
use super::{
    Session, SessionError, SessionValue,
    canonical_methods::{CanonicalMethods, InspectionContext},
    own, registry_lock,
};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicUsize},
};
use wasmtime::{AsContextMut, Func, HeapType, Instance, RootScope, Store, Val};

// Unrelated protocol frames do not revoke the enclosing dynamic IHash scope.
// Future write observation must resolve the nearest actual IHash frame, not
// merely inspect frames.last().active. No accessor here grants write authority.
struct Frame {
    // Root exact actual values through reentrant guest calls and GC. These roots
    // are private observer storage, not public external-value accounting.
    _receiver: SessionValue,
    _method: SessionValue,
    active: bool,
    nearest_ihash: Option<usize>,
}
struct State {
    authority: Option<Arc<CanonicalMethods>>,
    frames: Vec<Frame>,
    #[cfg(test)]
    trace: Option<Vec<Event>>,
}
impl State {
    fn nearest_ihash(&self) -> Option<&Frame> {
        self.frames
            .last()
            .and_then(|frame| frame.nearest_ihash)
            .and_then(|index| self.frames.get(index))
    }
}
pub(super) struct Observer {
    state: Mutex<State>,
    handles: Arc<AtomicUsize>,
}
#[cfg(test)]
#[derive(Clone, Debug)]
struct Event {
    enter: bool,
    active: bool,
    parent_active: bool,
    depth: usize,
    route: i32,
    nearest_ihash_depth: Option<usize>,
    receiver_matches_parent_ihash: Option<bool>,
}
struct Scope {
    observer: Arc<Observer>,
    route: i32,
}
impl Drop for Scope {
    fn drop(&mut self) {
        let mut state = registry_lock(&self.observer.state);
        let frame = state.frames.pop().expect("dispatch scope stack");
        #[cfg(test)]
        {
            let depth = state.frames.len();
            let parent_active = state.nearest_ihash().is_some();
            let nearest_ihash_depth = state.frames.last().and_then(|frame| frame.nearest_ihash);
            if let Some(trace) = &mut state.trace {
                trace.push(Event {
                    enter: false,
                    active: frame.active,
                    parent_active,
                    depth,
                    route: self.route,
                    nearest_ihash_depth,
                    receiver_matches_parent_ihash: None,
                });
            }
        }
        #[cfg(not(test))]
        let _ = (frame, self.route);
    }
}
impl Observer {
    // Snapshot actual IHash receivers only. No guest execution under the mutex;
    // the caller's RootScope keeps these references alive across native proof.
    // This is dynamic context, not an authorization or persistent allowlist.
    pub(super) fn receivers(&self, mut store: wasmtime::StoreContextMut<'_, ()>, irq: &AtomicBool)
        -> wasmtime::Result<Vec<Val>> {
        Session::consume_host_work(store.as_context_mut(), 2, irq)?;
        let state = registry_lock(&self.state);
        let mut count = 0usize;
        for frame in &state.frames {
            Session::consume_host_work(store.as_context_mut(), 1, irq)?;
            if frame.active { count += 1; }
        }
        let bytes = count.checked_mul(std::mem::size_of::<Val>())
            .and_then(|n| u64::try_from(n).ok())
            .ok_or_else(|| wasmtime::Error::msg("Dispatch proof context overflow"))?;
        Session::consume_host_work(store.as_context_mut(), bytes, irq)?;
        let mut receivers = Vec::new();
        receivers.try_reserve_exact(count)
            .map_err(|_| wasmtime::Error::msg("Dispatch proof context allocation failed"))?;
        for frame in &state.frames {
            Session::consume_host_work(store.as_context_mut(), 2, irq)?;
            if frame.active {
                receivers.push(Val::AnyRef(Some(frame._receiver.value.to_rooted(&mut store))));
            }
        }
        Ok(receivers)
    }

    pub(super) fn has_scope(&self) -> bool {
        registry_lock(&self.state).nearest_ihash().is_some()
    }
    // Returned root belongs to the caller's active Wasmtime rooting scope.
    pub(super) fn receiver(&self, mut store: wasmtime::StoreContextMut<'_, ()>) -> Option<Val> {
        registry_lock(&self.state).nearest_ihash().map(|frame|
            Val::AnyRef(Some(frame._receiver.value.to_rooted(&mut store))))
    }

    pub(super) fn seal(&self, authority: Arc<CanonicalMethods>) -> Result<(), SessionError> {
        let mut state = registry_lock(&self.state);
        if state.authority.is_some() {
            return Err(wasmtime::Error::msg("Dispatch observer already sealed").into());
        }
        state.authority = Some(authority);
        Ok(())
    }
}
fn raw(error: SessionError) -> wasmtime::Error {
    match error {
        SessionError::Host(e) | SessionError::Trap(e) => e,
        other => wasmtime::Error::msg(other.to_string()),
    }
}
pub(super) fn install(
    store: &mut Store<()>,
    runtime: Instance,
    identity: u64,
    interrupt: Arc<AtomicBool>,
) -> wasmtime::Result<Arc<Observer>> {
    let observer = Arc::new(Observer {
        state: Mutex::new(State {
            authority: None,
            frames: Vec::new(),
            #[cfg(test)]
            trace: None,
        }),
        handles: Arc::new(AtomicUsize::new(0)),
    });
    let setter = runtime
        .get_func(&mut *store, "macro-dispatch-observer-install")
        .ok_or_else(|| wasmtime::Error::msg("Missing Macro dispatch observer setter"))?;
    let parameter = setter.ty(&*store).params().next().unwrap();
    let HeapType::ConcreteFunc(ty) = parameter.unwrap_ref().heap_type() else {
        return Err(wasmtime::Error::msg(
            "Invalid dispatch observer callback type",
        ));
    };
    let invoke = runtime.get_func(&mut *store, "invoke").unwrap();
    let native_invoke = runtime
        .get_func(&mut *store, "protocol-native-invoke")
        .unwrap();
    let state = observer.clone();
    let hook = Func::new(
        &mut *store,
        ty.clone(),
        move |mut caller, arguments, results| {
            // Charge common host bookkeeping even before bootstrap seals authority.
            // This covers route validation, locks, and the optional authority clone;
            // key inspection and frame/root allocation are charged separately below.
            Session::consume_host_work(caller.as_context_mut(), 4, &interrupt)?;
            let route = arguments[4].unwrap_i32();
            if !(0..=2).contains(&route) {
                return Err(wasmtime::Error::msg("Invalid dispatch observation route"));
            }
            let authority = registry_lock(&state.state).authority.clone();
            // Bootstrap executes before authority capture; it cannot authenticate a
            // dispatch. Avoid staging roots while there is no outer active scope.
            let active = if let Some(authority) = authority {
                authority
                    .ihash_dispatch_matches_in_context(
                        InspectionContext {
                            store: caller.as_context_mut(),
                            identity,
                            runtime,
                            interrupt: &interrupt,
                        },
                        &arguments[2],
                        route == 2,
                    )
                    .map_err(raw)?
            } else {
                false
            };
            let needs_frame = active || registry_lock(&state.state).nearest_ihash().is_some();
            let _scope = if needs_frame {
                Session::consume_host_work(
                    caller.as_context_mut(),
                    3 + 2 * std::mem::size_of::<SessionValue>() as u64,
                    &interrupt,
                )?;
                let (receiver, method) = {
                    let mut scope = RootScope::new(&mut caller);
                    (
                        own(&mut scope, arguments[3].clone(), identity, &state.handles)
                            .map_err(raw)?,
                        own(&mut scope, arguments[0].clone(), identity, &state.handles)
                            .map_err(raw)?,
                    )
                };
                let mut locked = registry_lock(&state.state);
                if locked.frames.len() == locked.frames.capacity() {
                    let capacity = locked
                        .frames
                        .capacity()
                        .checked_mul(2)
                        .unwrap_or(usize::MAX)
                        .max(1);
                    let bytes = capacity
                        .checked_add(locked.frames.len())
                        .and_then(|n| n.checked_mul(std::mem::size_of::<Frame>()))
                        .and_then(|n| u64::try_from(n).ok())
                        .ok_or_else(|| {
                            wasmtime::Error::msg("Dispatch scope allocation overflow")
                        })?;
                    Session::consume_host_work(caller.as_context_mut(), bytes, &interrupt)?;
                    let extra = capacity - locked.frames.len();
                    locked
                        .frames
                        .try_reserve_exact(extra)
                        .map_err(|_| wasmtime::Error::msg("Dispatch scope allocation failed"))?;
                }
                // Unrelated callbacks inherit the nearest actual IHash scope.
                // Cache its index: no uncharged linear walk of the native stack.
                let nearest_ihash = if active {
                    Some(locked.frames.len())
                } else {
                    locked.frames.last().and_then(|frame| frame.nearest_ihash)
                };
                #[cfg(test)]
                {
                    let depth = locked.frames.len();
                    let parent_active = locked.nearest_ihash().is_some();
                    let nearest_ihash_depth = if active { Some(depth) } else { nearest_ihash };
                    let receiver_matches_parent_ihash = if let Some(parent) = locked.nearest_ihash()
                    {
                        let mut scope = RootScope::new(&mut caller);
                        let parent = parent._receiver.value.to_rooted(&mut scope);
                        let actual = receiver.value.to_rooted(&mut scope);
                        Some(wasmtime::Rooted::<wasmtime::AnyRef>::ref_eq(
                            &scope, &parent, &actual,
                        )?)
                    } else {
                        None
                    };
                    if let Some(trace) = &mut locked.trace {
                        trace.push(Event {
                            enter: true,
                            active,
                            parent_active,
                            depth,
                            route,
                            nearest_ihash_depth,
                            receiver_matches_parent_ihash,
                        });
                    }
                }
                locked.frames.push(Frame {
                    _receiver: receiver,
                    _method: method,
                    active,
                    nearest_ihash,
                });
                drop(locked);
                Some(Scope {
                    observer: state.clone(),
                    route,
                })
            } else {
                None
            };
            // These are invocation primitives, not dispatchers. Do not reselect a
            // method, re-evaluate arguments, translate exceptions, or restore dynamic
            // state here. Raw Wasmtime errors/pending exceptions pass to the caller.
            let function = if route == 1 { native_invoke } else { invoke };
            let _hash_samples=super::hash_profile::HashScope::new(active);
            function.call(&mut caller, &arguments[..2], results)
        },
    );
    setter.call(&mut *store, &[Val::FuncRef(Some(hook))], &mut [])?;
    Ok(observer)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::portable::{self, resolve::Phase};
    use wasmtime::{AnyRef, Module, Rooted};

    fn reset_trace(session: &Session) -> Arc<Observer> {
        let observer = session.dispatch_observer.clone().unwrap();
        let mut state = registry_lock(&observer.state);
        assert!(state.frames.is_empty());
        state.trace = Some(Vec::new());
        drop(state);
        observer
    }
    fn events(observer: &Observer) -> Vec<Event> {
        let state = registry_lock(&observer.state);
        assert!(
            state.frames.is_empty(),
            "RAII must leave no dispatch frames"
        );
        assert_eq!(
            observer.handles.load(std::sync::atomic::Ordering::Relaxed),
            0
        );
        state.trace.as_ref().unwrap().clone()
    }
    fn same(session: &mut Session, a: &SessionValue, b: &SessionValue) -> bool {
        session
            .inspect(a, |mut store, value| {
                let expected = b.value.to_rooted(&mut store);
                Rooted::<AnyRef>::ref_eq(&store, value.unwrap_anyref().unwrap(), &expected)
            })
            .unwrap()
    }
    fn display(session: &mut Session, value: &SessionValue) -> String {
        crate::portable_repl::display(session, value).unwrap()
    }
    const SETUP: &str = r#"
      (def dispatch-helper (fn [x] (count x)))
      (extend-type List ICounted (-count [self] (do (set! (.-__hash self) 91) 3)))
      (def dispatch-saved suss.core/-hash)
      (def dispatch-x (list 17))
      (extend-type List IHash (-hash [self] (do (dispatch-helper self) 91)))
    "#;

    #[test]
    fn ihash_dispatch_boundary_macro_variant_and_host_only_setter() {
        let engine = super::super::engine().unwrap();
        let ordinary = Module::new(&engine, crate::runtime_abi::module()).unwrap();
        let observed = Module::new(&engine, crate::runtime_abi::macro_module()).unwrap();
        assert_eq!(ordinary.imports().len(), 0);
        assert_eq!(observed.imports().len(), 0);
        assert!(
            !ordinary
                .exports()
                .any(|e| e.name() == "macro-dispatch-observer-install")
        );
        assert!(
            observed
                .exports()
                .any(|e| e.name() == "macro-dispatch-observer-install")
        );
        let mut session = Session::with_options_in(Default::default(), Phase::Macro).unwrap();
        // Even an import with the wrong signature must be rejected before link,
        // rather than exposing the one-time callback installer to guest code.
        let mut types = crate::runtime_abi::prelude();
        types.ty().function([], []);
        let mut imports = wasm_encoder::ImportSection::new();
        imports.import(
            "suss.runtime",
            "macro-dispatch-observer-install",
            wasm_encoder::EntityType::Function(crate::runtime_abi::TYPE_COUNT),
        );
        let mut module = wasm_encoder::Module::new();
        module
            .section(&types)
            .section(&imports)
            .section(&crate::runtime_abi::Manifest::default().section());
        let bytes = portable::artifact_identity::annotate_source(
            &module.finish(),
            Phase::Macro,
            None,
            Some(&[]),
        )
        .unwrap();
        let error = session
            .install(&[&bytes], Default::default(), &[])
            .unwrap_err();
        assert!(error.to_string().contains("host-only"), "{error}");
    }

    #[test]
    fn ihash_dispatch_boundary_live_replacement_helpers_and_rebinding() {
        let mut session = Session::new_macro().unwrap();
        session.eval(SETUP).unwrap();
        let saved = session.eval("dispatch-saved").unwrap();
        let current = session.eval("suss.core/-hash").unwrap();
        assert!(
            same(&mut session, &saved, &current),
            "no wrapper replaces the public dispatcher"
        );
        // Register exact source inputs; this observation infrastructure MUST NOT
        // authenticate even a legitimate nested-helper memo write yet.
        let original = session.eval("(list 17)").unwrap();
        let fields = session.data_fields(&original).unwrap();
        let class = session.eval("suss.core/List").unwrap();
        let args = fields.iter().collect::<Vec<_>>();
        let (registered, inputs) = session.data_construct_with_inputs(&class, &args).unwrap();
        let inputs = inputs.unwrap();
        let observer = reset_trace(&session);
        let f = session.eval("(fn [x] (dispatch-saved x))").unwrap();
        let result = session.invoke(&f, &[&registered]).unwrap();
        assert_eq!(display(&mut session, &result), "91");
        let trace = events(&observer);
        assert!(
            trace
                .iter()
                .any(|e| e.enter && e.active && e.depth == 0 && e.route == 0)
        );
        assert!(
            trace.iter().any(|e| e.enter
                && !e.active
                && e.parent_active
                && e.depth == 1
                && e.nearest_ihash_depth == Some(0)
                && e.receiver_matches_parent_ihash == Some(true)),
            "ICounted helper retains nearest outer IHash context while writing its memo: {trace:?}"
        );
        assert!(
            trace
                .iter()
                .any(|e| !e.enter && !e.active && e.parent_active)
        );
        assert!(
            !inputs.matches(&mut session, &registered).unwrap(),
            "no custom-write receipts yet"
        );
        reset_trace(&session);
        session.eval("(count dispatch-x)").unwrap();
        assert!(
            events(&observer).is_empty(),
            "unrelated protocol alone grants no scope"
        );
        let keyword = session.eval(":x").unwrap();
        let map = session.eval("{:x 7}").unwrap();
        let call_ifn = session.eval("(fn [k m] (k m))").unwrap();
        reset_trace(&session);
        let value = session.invoke(&call_ifn, &[&keyword, &map]).unwrap();
        assert_eq!(display(&mut session, &value), "7");
        assert!(
            events(&observer).is_empty(),
            "IFn adaptation does not enter the observer"
        );
        session.enter_namespace("suss.core").unwrap();
        session.eval("(def -hash (fn [_] 72))").unwrap();
        session.enter_namespace("user").unwrap();
        reset_trace(&session);
        let value = session.eval("(suss.core/-hash dispatch-x)").unwrap();
        assert_eq!(display(&mut session, &value), "72");
        assert!(
            events(&observer).is_empty(),
            "live public rebinding is respected"
        );
        let value = session.eval("(dispatch-saved dispatch-x)").unwrap();
        assert_eq!(display(&mut session, &value), "91");
        assert!(events(&observer).iter().any(|e| e.enter && e.active));
    }

    #[test]
    fn ihash_dispatch_boundary_raw_throw_nested_dispatch_and_trap_recovery() {
        let mut session = Session::new_macro().unwrap();
        session
            .eval(
                r#"
          (def dispatch-error (new js/Error "exact dispatch error"))
          (def dispatch-inner (list 2))
          (def dispatch-outer (list 1))
          (extend-type List IHash
            (-hash [self]
              (if (identical? self dispatch-outer)
                (suss.core/-hash dispatch-inner)
                (throw dispatch-error))))
        "#,
            )
            .unwrap();
        let observer = reset_trace(&session);
        let expected = session.eval("dispatch-error").unwrap();
        let failure = session
            .eval("(suss.core/-hash dispatch-outer)")
            .unwrap_err();
        let SessionError::Language(actual) = failure else {
            panic!("{failure:?}")
        };
        assert!(
            same(&mut session, &expected, &actual),
            "original thrown object crosses raw host boundary"
        );
        let trace = events(&observer);
        assert!(trace.iter().any(|e| e.enter
            && e.active
            && e.parent_active
            && e.depth == 1
            && e.receiver_matches_parent_ihash == Some(false)));
        let recovered = session.eval("(try (suss.core/-hash dispatch-outer) (catch :default e (identical? e dispatch-error)))").unwrap();
        assert_eq!(display(&mut session, &recovered), "true");
        events(&observer);
        session
            .eval("(extend-type List IHash (-hash [_] (loop [] (recur))))")
            .unwrap();
        reset_trace(&session);
        session.set_operation_fuel(10_000);
        let failure = session
            .eval("(suss.core/-hash dispatch-inner)")
            .unwrap_err();
        assert!(
            matches!(failure, SessionError::Trap(ref e) if e.downcast_ref::<wasmtime::Trap>() == Some(&wasmtime::Trap::OutOfFuel)),
            "{failure:?}"
        );
        assert!(
            events(&observer).iter().any(|e| e.enter && e.active),
            "must trap AFTER scope entry"
        );
        session.set_operation_fuel(crate::portable::DEFAULT_OPERATION_FUEL);
        session
            .eval("(extend-type List IHash (-hash [_] 83))")
            .unwrap();
        let value = session.eval("(suss.core/-hash dispatch-inner)").unwrap();
        assert_eq!(display(&mut session, &value), "83");
        events(&observer);
    }

    #[test]
    fn ihash_dispatch_boundary_native_and_legacy_routes_and_key_mutation() {
        let mut session = Session::new_macro().unwrap();
        session.eval(SETUP).unwrap();
        // Install through the native ABI to deliberately test differing arity:
        // source extend-type correctly rejects undeclared IHash arities. Native
        // dispatch must still pad this extra parameter with undefined.
        let dispatcher = session.eval("suss.core/-hash").unwrap();
        let method = session
            .eval("(fn [_ ignored] (if (undefined? ignored) 17 18))")
            .unwrap();
        let runtime = session.runtime;
        session
            .inspect(&dispatcher, |mut store, dispatcher| {
                let method = Val::AnyRef(Some(method.value.to_rooted(&mut store)));
                let setter = runtime
                    .get_func(&mut store, "protocol-native-method-set")
                    .unwrap();
                setter.call(
                    &mut store,
                    &[dispatcher, Val::I32(2), method],
                    &mut [Val::null_any_ref()],
                )
            })
            .unwrap();
        let observer = reset_trace(&session);
        session.eval("(suss.core/-hash 17)").unwrap();
        assert!(
            events(&observer)
                .iter()
                .any(|e| e.enter && e.active && e.route == 1)
        );
        let key_name = format!(
            "key-{}",
            "suss.core/IHash/-hash/1"
                .bytes()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        );
        let handles = Arc::new(AtomicUsize::new(0));
        let key = super::super::canonical_methods::binding(
            &mut session,
            "suss.internal.protocol-keys",
            &key_name,
            &handles,
        )
        .unwrap();
        let legacy = {
            let mut scope = RootScope::new(&mut session.store);
            let key_val = Val::AnyRef(Some(key.value.to_rooted(&mut scope)));
            let constructor = session
                .runtime
                .get_func(&mut scope, "protocol-dispatcher-new")
                .unwrap();
            let mut result = [Val::null_any_ref()];
            constructor
                .call(&mut scope, &[key_val], &mut result)
                .unwrap();
            own(&mut scope, result[0].clone(), session.identity, &handles).unwrap()
        };
        let receiver = session.eval("dispatch-x").unwrap();
        reset_trace(&session);
        session.invoke(&legacy, &[&receiver]).unwrap();
        assert!(
            events(&observer)
                .iter()
                .any(|e| e.enter && e.active && e.route == 2)
        );
        // Poison only the sealed key's metadata; actual dispatch still works,
        // but exact key identity without its storage witness is insufficient.
        let poison = session.eval("991").unwrap();
        session
            .inspect(&key, |mut store, value| {
                let key = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
                let poison = poison.value.to_rooted(&mut store);
                key.set_field(&mut store, 3, Val::AnyRef(Some(poison)))
            })
            .unwrap();
        reset_trace(&session);
        let result = session.invoke(&legacy, &[&receiver]).unwrap();
        assert_eq!(display(&mut session, &result), "91");
        assert!(
            events(&observer).is_empty(),
            "mutated key declines authority without suppressing dispatch"
        );
        let result = session.eval("(suss.core/-hash 17)").unwrap();
        assert_eq!(display(&mut session, &result), "17");
        assert!(
            events(&observer).is_empty(),
            "native fallback with damaged key also executes without authority"
        );
    }
}
