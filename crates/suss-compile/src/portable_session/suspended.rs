//! Synchronous access to uniquely borrowed, suspended ancestor Stores.
//!
//! A foreign guest may call back into any ancestor, not just its direct caller.
//! Selecting an ancestor removes it from the scoped registry until it returns;
//! only its new Wasmtime Caller can lend it again. No Store pointer, lifetime
//! extension, mutex guard across guest code or cross-thread Store access is used.
#![forbid(unsafe_code)]
use super::*;
use scoped_tls_hkt::scoped_thread_local;
use std::cell::Cell;

thread_local! {
    static RESTORING_DYNAMIC: Cell<bool> = const { Cell::new(false) };
}

/// Finite runtime frame/cell cleanup must finish before an interrupted ancestor
/// resumes. No source body, finally handler or foreign callback executes here.
/// Current-session requests retain their normal epoch behavior.
pub(super) fn restoring_dynamic<T>(operation: impl FnOnce() -> T) -> T {
    struct Restore(bool);
    impl Drop for Restore {
        fn drop(&mut self) {
            RESTORING_DYNAMIC.with(|state| state.set(self.0));
        }
    }
    let _restore = Restore(RESTORING_DYNAMIC.with(|state| state.replace(true)));
    operation()
}

trait Live {
    fn owner(&self) -> u64;
    fn interrupt(&self) -> &Arc<AtomicBool>;
    fn engine(&self) -> &Engine;
    fn run(&mut self, action: &mut dyn for<'s> FnMut(&mut SessionHost<'s>));
}
struct View<'a, 's>(&'a mut SessionHost<'s>);
impl Live for View<'_, '_> {
    fn owner(&self) -> u64 {
        self.0.identity
    }
    fn interrupt(&self) -> &Arc<AtomicBool> {
        &self.0.interrupt
    }
    fn engine(&self) -> &Engine {
        &self.0.engine
    }
    fn run(&mut self, action: &mut dyn for<'s> FnMut(&mut SessionHost<'s>)) {
        action(&mut self.0.reborrow());
    }
}
type Action<'a> = dyn for<'s> FnMut(&mut SessionHost<'s>, &mut dyn Registry) + 'a;
trait Registry {
    fn contains(&self, owner: u64) -> bool;
    fn observes_interrupt(&self, flag: &Arc<AtomicBool>, engine: &Engine) -> bool;
    fn interrupt_requested(&self) -> bool;
    fn choose(&mut self, owner: u64, action: &mut Action<'_>) -> bool;
}
struct Empty;
impl Registry for Empty {
    fn observes_interrupt(&self, _: &Arc<AtomicBool>, _: &Engine) -> bool {
        false
    }
    fn interrupt_requested(&self) -> bool {
        false
    }
    fn contains(&self, _: u64) -> bool {
        false
    }
    fn choose(&mut self, _: u64, _: &mut Action<'_>) -> bool {
        false
    }
}
struct Joined<'a> {
    head: &'a mut (dyn Live + 'a),
    rest: &'a mut (dyn Registry + 'a),
}
impl Registry for Joined<'_> {
    fn observes_interrupt(&self, flag: &Arc<AtomicBool>, engine: &Engine) -> bool {
        (Arc::ptr_eq(self.head.interrupt(), flag) && Engine::same(self.head.engine(), engine))
            || self.rest.observes_interrupt(flag, engine)
    }
    fn interrupt_requested(&self) -> bool {
        self.head.interrupt().load(Ordering::SeqCst) || self.rest.interrupt_requested()
    }
    fn contains(&self, owner: u64) -> bool {
        self.head.owner() == owner || self.rest.contains(owner)
    }
    fn choose(&mut self, owner: u64, action: &mut Action<'_>) -> bool {
        let head = &mut *self.head;
        let rest = &mut *self.rest;
        if head.owner() == owner {
            head.run(&mut |host| action(host, rest));
            true
        } else {
            rest.choose(owner, &mut |host, tail| {
                let mut remaining = Joined {
                    head: &mut *head,
                    rest: tail,
                };
                action(host, &mut remaining);
            })
        }
    }
}
scoped_thread_local!(static mut ACTIVE: for<'a> &'a mut (dyn Registry + 'a));

/// Lend a caller while a different Store executes. Scoped TLS restores the
/// previous registry on success, error and panic; no borrow escapes this call.
pub(crate) fn scope<T>(host: &mut SessionHost<'_>, action: impl FnOnce() -> T) -> T {
    let mut current = View(host);
    let mut action = Some(action);
    if ACTIVE.is_set() {
        ACTIVE.with(|rest| {
            assert!(
                !rest.contains(current.owner()),
                "a suspended Store must be selected before reentry"
            );
            let mut registry = Joined {
                head: &mut current,
                rest,
            };
            ACTIVE.set(&mut registry, || action.take().unwrap()())
        })
    } else {
        let mut empty = Empty;
        let mut registry = Joined {
            head: &mut current,
            rest: &mut empty,
        };
        ACTIVE.set(&mut registry, || action.take().unwrap()())
    }
}

/// Borrow one actual suspended ancestor, retaining all the other ancestors for
/// nested callbacks. The result type cannot contain the temporary Store borrow.
pub(crate) fn with_host<T>(
    owner: u64,
    action: impl for<'s> FnOnce(&mut SessionHost<'s>) -> T,
) -> Option<T> {
    let mut action = Some(action);
    let mut output = None;
    if ACTIVE.is_set() {
        ACTIVE.with(|registry| {
            registry.choose(owner, &mut |host, remainder| {
                ACTIVE.set(remainder, || output = Some(action.take().unwrap()(host)));
            });
        });
    }
    output
}

/// Exact flag identity matters: an unrelated session with the same boolean
/// state does not participate in this synchronous cancellation chain. The target
/// must also share the ancestor engine, so its epoch callback is woken directly.
pub(crate) fn observes_interrupt(flag: &Arc<AtomicBool>, engine: &Engine) -> bool {
    ACTIVE.is_set() && ACTIVE.with(|registry| registry.observes_interrupt(flag, engine))
}

/// Ancestors consume their own requests during unwinding. A descendant only
/// observes the request and exhausts its own current operation.
pub(crate) fn interrupt_requested() -> bool {
    !RESTORING_DYNAMIC.with(Cell::get)
        && ACTIVE.is_set()
        && ACTIVE.with(|registry| registry.interrupt_requested())
}

#[cfg(all(test, not(suss_generator)))]
mod tests {
    use super::*;
    use wasmtime::{Caller, Module, TypedFunc};

    struct Peer {
        owner: u64,
        run: TypedFunc<i32, i32>,
        available: Mutex<Option<Session>>,
    }
    type Peers = Arc<Mutex<Vec<Arc<Peer>>>>;
    struct Lease {
        peer: Arc<Peer>,
        host: Option<Session>,
    }
    impl Lease {
        fn take(peer: Arc<Peer>) -> Self {
            let host = registry_lock(&peer.available)
                .take()
                .expect("a suspended ancestor must be borrowed instead of leased twice");
            Self {
                peer,
                host: Some(host),
            }
        }
        fn host(&mut self) -> &mut Session {
            self.host.as_mut().unwrap()
        }
    }
    impl Drop for Lease {
        fn drop(&mut self) {
            let mut available = registry_lock(&self.peer.available);
            assert!(available.is_none());
            *available = self.host.take();
        }
    }
    fn error(error: SessionError) -> wasmtime::Error {
        match error {
            SessionError::Host(error) | SessionError::Trap(error) => error,
            error => wasmtime::Error::msg(error.to_string()),
        }
    }
    fn run(
        host: &mut SessionHost<'_>,
        function: &TypedFunc<i32, i32>,
        depth: i32,
        parent: u64,
    ) -> wasmtime::Result<(wasmtime::Result<i32>, u64)> {
        let saved = host.operation_fuel_remaining().map_err(error)?;
        let allowance = host
            .cap_operation_fuel(parent.min(host.options.fuel_per_operation))
            .map_err(error)?;
        let result = function.call(&mut host.store, depth);
        let spent = allowance - host.operation_fuel_remaining().map_err(error)?;
        host.settle_nested_fuel(saved, spent).map_err(error)?;
        Ok((result, spent))
    }
    fn invoke(
        peers: &Peers,
        index: usize,
        depth: i32,
        parent: u64,
    ) -> wasmtime::Result<(wasmtime::Result<i32>, u64)> {
        let peer = registry_lock(peers)[index].clone();
        if let Some(result) = with_host(peer.owner, |host| run(host, &peer.run, depth, parent)) {
            return result;
        }
        let mut lease = Lease::take(peer.clone());
        run(lease.host(), &peer.run, depth, parent)
    }
    fn root(
        peers: &Peers,
        depth: i32,
        allowance: u64,
    ) -> wasmtime::Result<(wasmtime::Result<i32>, u64)> {
        let endpoints = registry_lock(peers).clone();
        for peer in &endpoints {
            registry_lock(&peer.available)
                .as_mut()
                .unwrap()
                .store
                .set_fuel(1_000_000)?;
        }
        let peer = endpoints[0].clone();
        let mut lease = Lease::take(peer.clone());
        let host = lease.host();
        host.store.set_fuel(allowance)?;
        let result = peer.run.call(&mut host.store, depth);
        let spent = allowance - host.store.get_fuel()?;
        Ok((result, spent))
    }

    #[test]
    fn suspended_parent_interrupt_stops_a_foreign_guest_and_retires_both_budgets() {
        use std::sync::{Barrier, mpsc};
        let mut parent = Session::new().unwrap();
        let mut child = Session::new_macro().unwrap();
        let bridge = crate::portable_macro_data::FormBridge::new(&mut child).unwrap();
        let context = child
            .enter_evaluation_math_context(EvaluationMathContext::Context(3, 4), &bridge)
            .unwrap();
        let flag = parent.host_work_interrupt();
        let parent_engine = parent.engine.clone();
        let handle = parent.interrupt_handle();
        let (started, start) = mpsc::channel();
        let rendezvous = Arc::new(Barrier::new(2));
        let release = rendezvous.clone();
        let mut linker = Linker::new(&child.engine);
        linker
            .func_wrap("host", "started", move || -> wasmtime::Result<()> {
                started
                    .send(())
                    .map_err(|e| wasmtime::Error::msg(e.to_string()))?;
                release.wait();
                Ok(())
            })
            .unwrap();
        let module = Module::new(
            &child.engine,
            r#"(module
          (import "host" "started" (func $started))
          (func (export "go") (result eqref) call $started (loop $spin br $spin) unreachable))"#,
        )
        .unwrap();
        let instance = linker.instantiate(&mut child.store, &module).unwrap();
        let go = instance.get_func(&mut child.store, "go").unwrap();
        parent.store.set_fuel(1_000_000).unwrap();
        child.store.set_fuel(10_000_000).unwrap();
        let worker = std::thread::spawn(move || {
            start
                .recv_timeout(std::time::Duration::from_secs(5))
                .unwrap();
            handle.interrupt();
            rendezvous.wait();
        });
        let result = scope(&mut parent, || {
            assert!(observes_interrupt(&flag, &parent_engine));
            let outcome = {
                let mut roots = RootScope::new(&mut child.store);
                call(
                    &mut roots,
                    child.runtime,
                    go,
                    &[],
                    child.identity,
                    &child.handles,
                )
            };
            let restoration = child.leave_evaluation_math_context(&context);
            assert!(
                restoration.is_ok(),
                "finite child frame restoration must finish while the ancestor request remains pending: {restoration:?}"
            );
            outcome
        });
        worker.join().unwrap();
        assert!(result.unwrap_err().is_interrupt());
        assert_eq!(child.store.get_fuel().unwrap(), 0);
        assert!(
            flag.load(Ordering::SeqCst),
            "child must not consume its ancestor's request"
        );
        assert!(parent.charge_host_work(0).unwrap_err().is_interrupt());
        assert_eq!(parent.store.get_fuel().unwrap(), 0);
        assert!(!flag.load(Ordering::SeqCst));
        assert!(!ACTIVE.is_set());
        // A restored child must not retain the interrupted operation's frame.
        child.store.set_fuel(1_000_000).unwrap();
        let value = child.eval("*math-context*").unwrap();
        assert!(matches!(
            bridge.read(&mut child, &value, 0..0).unwrap().kind,
            suss_reader::forms::Kind::Nil
        ));
        for host in [&mut parent, &mut child] {
            let value = host.eval("42").unwrap();
            assert_eq!(
                host.inspect(&value, |mut store, value| runtime_number(
                    &mut store, &value
                ))
                .unwrap(),
                Some(RuntimeNumber::Long(42))
            );
        }
    }

    #[test]
    fn suspended_parent_interrupt_is_also_observed_by_foreign_host_work() {
        let mut parent = Session::new().unwrap();
        let mut child = Session::with_options_in(SessionOptions::default(), Phase::Macro).unwrap();
        let flag = parent.host_work_interrupt();
        let parent_engine = parent.engine.clone();
        let handle = parent.interrupt_handle();
        let unrelated = Arc::new(AtomicBool::new(false));
        let foreign_engine = Engine::default();
        parent.store.set_fuel(1_000_000).unwrap();
        child.store.set_fuel(1_000_000).unwrap();
        let result = scope(&mut parent, || {
            assert!(observes_interrupt(&flag, &parent_engine));
            assert!(!observes_interrupt(&unrelated, &parent_engine));
            assert!(
                !observes_interrupt(&flag, &foreign_engine),
                "foreign engines need their own epoch relay"
            );
            handle.interrupt();
            child.charge_host_work(1)
        });
        assert!(result.unwrap_err().is_interrupt());
        assert_eq!(child.store.get_fuel().unwrap(), 0);
        assert!(flag.load(Ordering::SeqCst));
        assert!(parent.charge_host_work(0).unwrap_err().is_interrupt());
        assert!(!flag.load(Ordering::SeqCst));
        assert_eq!(parent.store.get_fuel().unwrap(), 0);
        assert!(!observes_interrupt(&flag, &parent_engine));
        let value = child.eval("42").unwrap();
        assert_eq!(
            child
                .inspect(&value, |mut store, value| runtime_number(
                    &mut store, &value
                ))
                .unwrap(),
            Some(RuntimeNumber::Long(42))
        );
    }

    #[test]
    fn suspended_ancestors_reenter_three_actual_stores_and_charge_guest_work_once() {
        let peers: Peers = Arc::new(Mutex::new(Vec::new()));
        for (index, phase) in [Phase::Runtime, Phase::Macro, Phase::Macro]
            .into_iter()
            .enumerate()
        {
            let mut host = Session::with_options_in(
                SessionOptions {
                    fuel_per_operation: if index == 0 { 1_000_000 } else { 10_000 },
                    ..SessionOptions::default()
                },
                phase,
            )
            .unwrap();
            let owner = host.native_owner();
            let mut linker = Linker::new(&host.engine);
            let profile = host.reentry_profile();
            let registry = Arc::downgrade(&peers);
            linker
                .func_wrap(
                    "host",
                    "other",
                    move |mut caller: Caller<'_, ()>, depth: i32| -> wasmtime::Result<i32> {
                        let peers = registry.upgrade().unwrap();
                        let mut view = profile.bind(caller.as_context_mut());
                        let saved = view.operation_fuel_remaining().map_err(error)?;
                        let (result, spent) =
                            scope(&mut view, || invoke(&peers, (index + 1) % 3, depth, saved))?;
                        view.settle_nested_fuel(saved, spent).map_err(error)?;
                        result
                    },
                )
                .unwrap();
            let wat = format!(
                r#"(module
              (import "host" "other" (func $other (param i32) (result i32)))
              (func (export "run") (param $depth i32) (result i32)
                local.get $depth i32.eqz
                if (result i32) i32.const {}
                else local.get $depth i32.const 1 i32.sub call $other i32.const 1 i32.add end))"#,
                (index + 1) * 10
            );
            let module = Module::new(&host.engine, wat).unwrap();
            let instance = linker.instantiate(&mut host.store, &module).unwrap();
            let run = instance
                .get_typed_func::<i32, i32>(&mut host.store, "run")
                .unwrap();
            registry_lock(&peers).push(Arc::new(Peer {
                owner,
                run,
                available: Mutex::new(Some(host)),
            }));
        }
        let mut costs = Vec::new();
        for depth in [0, 1, 2, 3, 8, 16, 32, 64] {
            let (value, spent) = root(&peers, depth, 1_000_000).unwrap();
            assert_eq!(value.unwrap(), (depth % 3 + 1) * 10 + depth);
            assert!(!ACTIVE.is_set());
            assert!(
                registry_lock(&peers)
                    .iter()
                    .all(|p| registry_lock(&p.available).is_some())
            );
            costs.push((depth as u64, spent));
        }
        let base = costs[0].1;
        let step = costs[1].1 - base;
        assert!(step > 0);
        for (depth, spent) in costs {
            assert_eq!(
                spent,
                base + step * depth,
                "guest work must be charged exactly once"
            );
        }
        let (result, _) = root(&peers, 64, 100).unwrap();
        assert_eq!(
            result.unwrap_err().downcast_ref::<wasmtime::Trap>(),
            Some(&wasmtime::Trap::OutOfFuel)
        );
        assert!(!ACTIVE.is_set());
        assert_eq!(root(&peers, 2, 1_000_000).unwrap().0.unwrap(), 32);
    }

    #[test]
    fn suspended_selection_retires_on_panic_and_preserves_roots_and_reset_identity() {
        let mut first = Session::new().unwrap();
        let mut second = Session::with_options_in(SessionOptions::default(), Phase::Macro).unwrap();
        let owner = first.native_owner();
        let other = second.native_owner();
        let root = first
            .data_scalar(&portable::hir::Literal::Long(1 << 40))
            .unwrap();
        let handles = first.stats().external_value_handles;
        first.store.set_fuel(1_000_000).unwrap();
        second.store.set_fuel(1_000_000).unwrap();
        let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            scope(&mut first, || {
                scope(&mut second, || {
                    with_host(owner, |host| {
                        assert!(
                            with_host(owner, |_| ()).is_none(),
                            "selected host cannot be borrowed twice"
                        );
                        host.check(&root).unwrap();
                        with_host(other, |other| {
                            assert!(matches!(
                                other.check(&root),
                                Err(SessionError::ForeignValue)
                            ))
                        })
                        .unwrap();
                        panic!("controlled scope unwind");
                    })
                    .unwrap();
                })
            });
        }));
        assert!(panic.is_err());
        assert!(!ACTIVE.is_set());
        assert_eq!(first.stats().external_value_handles, handles);
        first.collect().unwrap();
        assert_eq!(
            first
                .inspect(&root, |mut store, value| runtime_number(&mut store, &value))
                .unwrap(),
            Some(RuntimeNumber::Long(1 << 40))
        );
        drop(root);
        first.reset().unwrap();
        assert_ne!(first.native_owner(), owner);
        scope(&mut first, || assert!(with_host(owner, |_| ()).is_none()));
        assert!(!ACTIVE.is_set());
    }
}
