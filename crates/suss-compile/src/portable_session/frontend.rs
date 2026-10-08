//! Native frontend observations; no source invocation, implicit await or replay.
//! Deliberately unlinked until the Session owner integrates this child module.
use super::*;

/// Explicit observation roots a terminal payload only for Ready or Failed.
#[derive(Debug)]
pub enum FutureState {
    Pending,
    Ready(SessionValue),
    Failed(SessionValue),
    Cancelled,
}

/// Payload-free observation for opaque native display of a nominal future.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FutureStatus {
    Pending,
    Ready,
    Failed,
    Cancelled,
}

/// Successful pure observation, not a recovery receipt for a dispatched turn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AsyncDispatch {
    NoDispatchNeeded,
    Required,
}

const NATIVE_SCOPE: i32 = 0;

fn export(
    scope: &mut RootScope<&mut Store<()>>,
    runtime: Instance,
    name: &str,
) -> Result<Func, SessionError> {
    runtime.get_func(&mut *scope, name).ok_or_else(|| {
        wasmtime::Error::msg(format!("Missing frontend runtime export {name}")).into()
    })
}

fn nominal_status(
    scope: &mut RootScope<&mut Store<()>>,
    runtime: Instance,
    value: Val,
) -> Result<Option<FutureStatus>, SessionError> {
    let mut result = [Val::I32(0)];
    export(scope, runtime, "future-is")?.call(&mut *scope, &[value], &mut result)?;
    if result[0].unwrap_i32() == 0 {
        return Ok(None);
    }
    export(scope, runtime, "future-status")?.call(&mut *scope, &[value], &mut result)?;
    Ok(Some(match result[0].unwrap_i32() {
        0 => FutureStatus::Pending,
        1 => FutureStatus::Ready,
        2 => FutureStatus::Failed,
        3 => FutureStatus::Cancelled,
        status => {
            return Err(
                wasmtime::Error::msg(format!("Invalid nominal future status {status}")).into(),
            );
        }
    }))
}

fn count(value: Val) -> Result<usize, SessionError> {
    usize::try_from(value.unwrap_i32())
        .map_err(|_| wasmtime::Error::msg("Negative runtime async count").into())
}

impl Session {
    /// Display-only nominal leaf probes, called inside the decoder's existing
    /// rooted inspection scope. No payload roots or source callbacks are made.
    pub(crate) fn display_runtime_exports(&mut self) -> Result<(Func, Func, Func), SessionError> {
        self.budget()?;
        let future_is = self
            .runtime
            .get_func(&mut self.store, "future-is")
            .ok_or_else(|| wasmtime::Error::msg("Missing future display predicate"))?;
        let future_status = self
            .runtime
            .get_func(&mut self.store, "future-status")
            .ok_or_else(|| wasmtime::Error::msg("Missing future display status"))?;
        let stream_kind = self
            .runtime
            .get_func(&mut self.store, "stream-value-kind")
            .ok_or_else(|| wasmtime::Error::msg("Missing stream display predicate"))?;
        Ok((future_is, future_status, stream_kind))
    }
    /// Observe nominal storage without running a waiter. Non-futures return None;
    /// values from another Session/generation return ForeignValue.
    pub fn future_state(
        &mut self,
        value: &SessionValue,
    ) -> Result<Option<FutureState>, SessionError> {
        self.check(value)?;
        self.frontend_scope(|scope, runtime, identity, handles| {
            let value = Val::AnyRef(Some(value.value.to_rooted(&mut *scope)));
            Ok(match nominal_status(scope, runtime, value)? {
                None => None,
                Some(FutureStatus::Pending) => Some(FutureState::Pending),
                Some(FutureStatus::Cancelled) => Some(FutureState::Cancelled),
                Some(status @ (FutureStatus::Ready | FutureStatus::Failed)) => {
                    let mut payload = [Val::AnyRef(None)];
                    export(scope, runtime, "future-result")?.call(
                        &mut *scope,
                        &[value],
                        &mut payload,
                    )?;
                    let payload = own(scope, payload[0], identity, handles)?;
                    Some(if status == FutureStatus::Ready {
                        FutureState::Ready(payload)
                    } else {
                        FutureState::Failed(payload)
                    })
                }
            })
        })
    }

    /// Observe a nominal future's status without creating a payload handle.
    pub fn future_status(
        &mut self,
        value: &SessionValue,
    ) -> Result<Option<FutureStatus>, SessionError> {
        self.check(value)?;
        self.frontend_scope(|scope, runtime, _, _| {
            let value = Val::AnyRef(Some(value.value.to_rooted(&mut *scope)));
            nominal_status(scope, runtime, value)
        })
    }

    pub fn is_future(&mut self, value: &SessionValue) -> Result<bool, SessionError> {
        self.check(value)?;
        self.frontend_scope(|scope, runtime, _, _| {
            let value = Val::AnyRef(Some(value.value.to_rooted(&mut *scope)));
            let mut result = [Val::I32(0)];
            export(scope, runtime, "future-is")?.call(&mut *scope, &[value], &mut result)?;
            Ok(result[0].unwrap_i32() != 0)
        })
    }

    /// Unique pending owners in NativeScope0, including an active owner. The
    /// snapshot remains independently rooted if subsequent requests mutate rows.
    pub fn pending_task_snapshot(&mut self) -> Result<Vec<SessionValue>, SessionError> {
        self.frontend_scope(|scope, runtime, identity, handles| {
            let mut result = [Val::AnyRef(None)];
            export(scope, runtime, "async-invocation-owner-snapshot")?.call(
                &mut *scope,
                &[Val::I32(NATIVE_SCOPE)],
                &mut result,
            )?;
            let array = result[0]
                .unwrap_anyref()
                .unwrap()
                .as_array(&*scope)?
                .ok_or_else(|| wasmtime::Error::msg("Invalid pending owner snapshot"))?;
            let mut owners = Vec::new();
            for index in 0..array.len(&*scope)? {
                let value = array.get(&mut *scope, index)?;
                owners.push(own(scope, value, identity, handles)?);
            }
            Ok(owners)
        })
    }

    /// Observe every global scheduler/stream root and this Session's native
    /// registry. No callback, settlement, pruning or cancellation hook runs.
    /// The Store owner must not mutate it between this observation and dispatch.
    /// Cancellation/start/yield readiness, rooted stream journals and consumed
    /// native hooks conservatively require dispatch. Not all blocked cleanup
    /// states qualify as idle, even when no source callback can currently run.
    pub fn observe_async_dispatch(&mut self) -> Result<AsyncDispatch, SessionError> {
        let requests = registry_lock(&self.pending_host_futures).clone();
        #[cfg(test)]
        let interrupt = self.interrupt_handle();
        self.frontend_scope(|scope, runtime, _, _| {
            #[cfg(test)]
            if tests::INTERRUPT_OBSERVATION.with(|flag| flag.replace(false)) {
                interrupt.interrupt();
            }
            let mut result = [Val::I32(0)];
            export(scope, runtime, "async-scheduler-dispatch-needed")?.call(
                &mut *scope,
                &[],
                &mut result,
            )?;
            if result[0].unwrap_i32() != 0 {
                return Ok(AsyncDispatch::Required);
            }
            let status = export(scope, runtime, "future-status")?;
            for request in requests {
                // A consumed cancellation hook may precede interrupted settlement.
                // Retain its uncertain retirement work rather than declaring idle.
                if registry_lock(&request.cancel).is_none() {
                    return Ok(AsyncDispatch::Required);
                }
                let future = request.root.to_rooted(&mut *scope);
                status.call(&mut *scope, &[Val::AnyRef(Some(future))], &mut result)?;
                if result[0].unwrap_i32() != 0 {
                    return Ok(AsyncDispatch::Required);
                }
            }
            Ok(AsyncDispatch::NoDispatchNeeded)
        })
    }

    /// NativeScope0 (pending unique owners, queued tokens, active callbacks).
    pub fn async_task_counts(&mut self) -> Result<(usize, usize, usize), SessionError> {
        self.frontend_scope(|scope, runtime, _, _| {
            let mut result = [Val::I32(0); 3];
            export(scope, runtime, "async-invocation-counts")?.call(
                &mut *scope,
                &[Val::I32(NATIVE_SCOPE)],
                &mut result,
            )?;
            Ok((count(result[0])?, count(result[1])?, count(result[2])?))
        })
    }

    /// Rooted stream operations still awaiting commit, settlement or retirement.
    /// Separate from task counts because trapped task bodies do not run finally.
    pub fn pending_stream_operation_count(&mut self) -> Result<usize, SessionError> {
        self.frontend_scope(|scope, runtime, _, _| {
            let mut result = [Val::I32(0)];
            export(scope, runtime, "stream-pending-count")?.call(&mut *scope, &[], &mut result)?;
            count(result[0])
        })
    }

    /// NativeScope0 cancellation-latched owners still awaiting retirement.
    pub fn async_retiring_count(&mut self) -> Result<usize, SessionError> {
        self.frontend_scope(|scope, runtime, _, _| {
            let mut result = [Val::I32(0)];
            export(scope, runtime, "async-invocation-retiring-count")?.call(
                &mut *scope,
                &[Val::I32(NATIVE_SCOPE)],
                &mut result,
            )?;
            count(result[0])
        })
    }

    /// Request a cancellation wave over a unique NativeScope0 owner snapshot.
    /// Accepted requests do not imply terminal cleanup; no source callback runs.
    /// Partial fuel-interrupted requests persist and are safe to retry.
    pub fn request_cancel_pending_tasks(&mut self) -> Result<usize, SessionError> {
        self.frontend_scope(|scope, runtime, _, _| {
            let mut result = [Val::I32(0)];
            export(scope, runtime, "async-cancel-invocation")?.call(
                &mut *scope,
                &[Val::I32(NATIVE_SCOPE)],
                &mut result,
            )?;
            count(result[0])
        })
    }

    /// Runtime reference identity, with both Session/generation checks first.
    pub fn values_identical(
        &mut self,
        a: &SessionValue,
        b: &SessionValue,
    ) -> Result<bool, SessionError> {
        self.check(a)?;
        self.check(b)?;
        self.frontend_scope(|scope, _, _, _| {
            let a = a.value.to_rooted(&mut *scope);
            let b = b.value.to_rooted(&mut *scope);
            Ok(wasmtime::Rooted::ref_eq(&*scope, &a, &b)?)
        })
    }

    fn frontend_scope<R>(
        &mut self,
        f: impl FnOnce(
            &mut RootScope<&mut Store<()>>,
            Instance,
            u64,
            &Arc<AtomicUsize>,
        ) -> Result<R, SessionError>,
    ) -> Result<R, SessionError> {
        self.budget()?;
        let mut scope = RootScope::new(&mut self.store);
        let checkpoint = dynamic_checkpoint(&mut scope, self.runtime)?;
        let outcome = f(&mut scope, self.runtime, self.identity, &self.handles);
        let outcome = match outcome {
            Err(SessionError::Trap(error) | SessionError::Host(error)) => Err(execution_error(
                &mut scope,
                self.runtime,
                error,
                self.identity,
                &self.handles,
            )),
            result => result,
        };
        let restoration = restore_dynamic(&mut scope, self.runtime, &checkpoint);
        match outcome {
            Err(original) => Err(original),
            Ok(value) => restoration.map(|()| value),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    thread_local! { pub(super) static INTERRUPT_OBSERVATION: std::cell::Cell<bool> = const { std::cell::Cell::new(false) }; }

    fn waiting() -> (Session, SessionValue, SessionValue) {
        let mut session = Session::new().unwrap();
        let gate = session.eval("(suss.internal.async/pending)").unwrap();
        let make = session
            .eval("(fn [gate] (suss.async/future* (suss.async/await* gate)))")
            .unwrap();
        let owner = session.invoke(&make, &[&gate]).unwrap();
        assert_eq!(
            session.observe_async_dispatch().unwrap(),
            AsyncDispatch::Required
        );
        assert!(session.run_async_turn().unwrap());
        assert_eq!(session.async_task_counts().unwrap(), (1, 0, 0));
        assert_eq!(
            session.observe_async_dispatch().unwrap(),
            AsyncDispatch::NoDispatchNeeded
        );
        (session, gate, owner)
    }

    // Deliberately install a valid terminal future outcome without scheduler
    // refresh, modeling the committed-storage/pre-refresh interruption boundary.
    fn publish_without_refresh(session: &mut Session, gate: &SessionValue, payload: &SessionValue) {
        let payload = payload.value.clone();
        session
            .inspect(gate, |mut store, value| {
                let object = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
                let fields = object
                    .field(&mut store, 1)?
                    .unwrap_anyref()
                    .unwrap()
                    .as_array(&store)?
                    .unwrap();
                let snapshot = fields
                    .get(&mut store, 0)?
                    .unwrap_anyref()
                    .unwrap()
                    .as_array(&store)?
                    .unwrap();
                let payload = payload.to_rooted(&mut store);
                snapshot.set(&mut store, 1, Val::AnyRef(Some(payload)))?;
                let ready =
                    wasmtime::AnyRef::from_i31(&mut store, wasmtime::I31::new_u32(1).unwrap());
                snapshot.set(&mut store, 0, Val::AnyRef(Some(ready)))?;
                Ok(())
            })
            .unwrap();
    }

    #[test]
    fn idle_dispatch_observation_interrupt_is_real_and_preserves_pending_roots() {
        let (mut session, _, owner) = waiting();
        INTERRUPT_OBSERVATION.with(|flag| flag.set(true));
        let error = session.observe_async_dispatch().unwrap_err();
        assert!(error.is_interrupt());
        session.collect().unwrap();
        assert_eq!(
            session.future_status(&owner).unwrap(),
            Some(FutureStatus::Pending)
        );
        assert_eq!(session.async_task_counts().unwrap(), (1, 0, 0));
        assert_eq!(
            session.observe_async_dispatch().unwrap(),
            AsyncDispatch::NoDispatchNeeded
        );
    }

    #[test]
    fn ready_dependency_without_enqueued_turn_still_requires_dispatch() {
        let (mut session, gate, owner) = waiting();
        let payload = session.eval("42").unwrap();
        publish_without_refresh(&mut session, &gate, &payload);
        assert_eq!(
            session.future_status(&gate).unwrap(),
            Some(FutureStatus::Ready)
        );
        assert_eq!(session.async_task_counts().unwrap(), (1, 0, 0));
        assert_eq!(
            session.observe_async_dispatch().unwrap(),
            AsyncDispatch::Required
        );
        assert!(session.run_async_turn().unwrap());
        assert_eq!(
            session.future_status(&owner).unwrap(),
            Some(FutureStatus::Ready)
        );
    }

    #[test]
    fn cancellation_and_yielded_work_cannot_be_declared_idle() {
        let (mut session, _, owner) = waiting();
        session.request_cancel_pending_tasks().unwrap();
        assert_eq!(
            session.observe_async_dispatch().unwrap(),
            AsyncDispatch::Required
        );
        assert!(session.run_async_turn().unwrap());
        assert_eq!(
            session.future_status(&owner).unwrap(),
            Some(FutureStatus::Cancelled)
        );
        let yielded = session
            .eval("(suss.async/future* (loop [i 0] (if (< i 10000) (recur (+ i 1)) i)))")
            .unwrap();
        assert!(session.run_async_turn().unwrap());
        assert_eq!(
            session.future_status(&yielded).unwrap(),
            Some(FutureStatus::Pending)
        );
        assert_eq!(
            session.observe_async_dispatch().unwrap(),
            AsyncDispatch::Required
        );
    }

    #[test]
    fn stream_journal_roots_require_service_even_without_source_queue() {
        let mut session = Session::new().unwrap();
        session.eval("(def pair (suss.internal.async/stream-pair 1)) (def reader (suss.internal.async/stream-pair-reader pair)) (def gate (suss.internal.async/pending)) (def read-op nil) (def reading (suss.async/future* (set! read-op (suss.internal.async/stream-begin-read reader 1)) (suss.async/await* gate)))").unwrap();
        assert!(session.run_async_turn().unwrap());
        let operation = session.eval("read-op").unwrap();
        let owner = session.eval("reading").unwrap();
        let completion = session
            .eval("(suss.internal.async/stream-operation-future read-op)")
            .unwrap();
        assert_eq!(
            session.future_status(&owner).unwrap(),
            Some(FutureStatus::Pending)
        );
        assert_eq!(
            session.future_status(&completion).unwrap(),
            Some(FutureStatus::Pending)
        );
        assert_eq!(session.async_task_counts().unwrap(), (1, 0, 0));
        assert_eq!(session.pending_stream_operation_count().unwrap(), 1);
        // Produce a real prepared withdrawal journal through the same runtime
        // operation used by service. Stop before snapshot commit/settlement;
        // this is an explicit journal fixture, not a claim about a fuel window.
        session.budget().unwrap();
        let withdraw = session
            .runtime
            .get_func(&mut session.store, "stream-withdraw-op")
            .unwrap();
        session
            .inspect(&operation, |mut store, value| {
                let object = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
                let fields = object
                    .field(&mut store, 1)?
                    .unwrap_anyref()
                    .unwrap()
                    .as_array(&store)?
                    .unwrap();
                assert_eq!(
                    fields
                        .get(&mut store, 6)?
                        .unwrap_anyref()
                        .unwrap()
                        .as_i31(&store)?
                        .unwrap()
                        .get_u32(),
                    1
                );
                withdraw.call(&mut store, &[value], &mut [])?;
                assert_eq!(
                    fields
                        .get(&mut store, 6)?
                        .unwrap_anyref()
                        .unwrap()
                        .as_i31(&store)?
                        .unwrap()
                        .get_u32(),
                    2
                );
                let old_value = fields.get(&mut store, 7)?;
                let new_value = fields.get(&mut store, 8)?;
                let old = old_value.unwrap_anyref().unwrap();
                let new = new_value.unwrap_anyref().unwrap();
                assert!(
                    !wasmtime::Rooted::<wasmtime::AnyRef>::ref_eq(&store, old, new)?,
                    "prepared withdrawal has a distinct unpublished snapshot"
                );
                Ok(())
            })
            .unwrap();
        assert_eq!(
            session.async_task_counts().unwrap(),
            (1, 0, 0),
            "no queued or active scheduler work"
        );
        assert_eq!(
            session.pending_stream_operation_count().unwrap(),
            1,
            "journal remains rooted"
        );
        assert_eq!(
            session.observe_async_dispatch().unwrap(),
            AsyncDispatch::Required
        );
        session
            .inspect(&operation, |mut store, value| {
                let object = value.unwrap_anyref().unwrap().as_struct(&store)?.unwrap();
                let fields = object
                    .field(&mut store, 1)?
                    .unwrap_anyref()
                    .unwrap()
                    .as_array(&store)?
                    .unwrap();
                assert_eq!(
                    fields
                        .get(&mut store, 6)?
                        .unwrap_anyref()
                        .unwrap()
                        .as_i31(&store)?
                        .unwrap()
                        .get_u32(),
                    2,
                    "pure probe must not commit the journal"
                );
                Ok(())
            })
            .unwrap();
        assert!(
            !session.run_async_turn().unwrap(),
            "service runs without resuming source waiting on gate"
        );
        assert_eq!(
            session.future_status(&completion).unwrap(),
            Some(FutureStatus::Cancelled)
        );
        assert_eq!(
            session.future_status(&owner).unwrap(),
            Some(FutureStatus::Pending)
        );
        assert_eq!(session.pending_stream_operation_count().unwrap(), 0);
        assert_eq!(
            session.observe_async_dispatch().unwrap(),
            AsyncDispatch::NoDispatchNeeded
        );
    }

    #[test]
    fn terminal_native_registry_requires_housekeeping_without_runnable_source() {
        let mut session = Session::new().unwrap();
        let closed = Arc::new(AtomicUsize::new(0));
        let observed = closed.clone();
        let future = session
            .pending_future_with_cancel(move || {
                observed.fetch_add(1, Ordering::SeqCst);
            })
            .unwrap();
        assert_eq!(
            session.observe_async_dispatch().unwrap(),
            AsyncDispatch::NoDispatchNeeded
        );
        let payload = session.eval("42").unwrap();
        publish_without_refresh(&mut session, &future, &payload);
        assert_eq!(session.async_task_counts().unwrap(), (0, 0, 0));
        assert_eq!(
            session.observe_async_dispatch().unwrap(),
            AsyncDispatch::Required
        );
        session.run_async_turn().unwrap();
        assert_eq!(closed.load(Ordering::SeqCst), 1);
        assert_eq!(session.stats().pending_host_requests, 0);
        assert_eq!(
            session.observe_async_dispatch().unwrap(),
            AsyncDispatch::NoDispatchNeeded
        );
    }
}
