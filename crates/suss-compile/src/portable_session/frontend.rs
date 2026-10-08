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
