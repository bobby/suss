//! Synchronous access to uniquely borrowed, suspended ancestor Stores.
//!
//! A foreign guest may call back into any ancestor, not just its direct caller.
//! Selecting an ancestor removes it from the scoped registry until it returns;
//! only its new Wasmtime Caller can lend it again. No Store pointer, lifetime
//! extension, mutex guard across guest code or cross-thread Store access is used.
#![forbid(unsafe_code)]
use super::*;
use scoped_tls_hkt::scoped_thread_local;

trait Live {
    fn owner(&self) -> u64;
    fn run(&mut self, action: &mut dyn for<'s> FnMut(&mut SessionHost<'s>));
}
struct View<'a, 's>(&'a mut SessionHost<'s>);
impl Live for View<'_, '_> {
    fn owner(&self) -> u64 {
        self.0.identity
    }
    fn run(&mut self, action: &mut dyn for<'s> FnMut(&mut SessionHost<'s>)) {
        action(&mut self.0.reborrow());
    }
}
type Action<'a> = dyn for<'s> FnMut(&mut SessionHost<'s>, &mut dyn Registry) + 'a;
trait Registry {
    fn contains(&self, owner: u64) -> bool;
    fn choose(&mut self, owner: u64, action: &mut Action<'_>) -> bool;
}
struct Empty;
impl Registry for Empty {
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
