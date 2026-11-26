use super::*;

/// Captured nominal form-data authority for a callback attached to the same
/// Session Store. This crate-private handle never accepts cross-Store values;
/// capture checks the bridge's existing owner before installing a callback.
/// The callback's surrounding evaluation scope owns dynamic-frame restoration.
pub(crate) struct BorrowedFormReader {
    core: Arc<BridgeCore>,
    invoke_method: wasmtime::Func,
    native_kind: wasmtime::Func,
    field_get: wasmtime::Func,
    undefined: Option<wasmtime::OwnedRooted<AnyRef>>,
    interrupt: Arc<std::sync::atomic::AtomicBool>,
}
impl FormBridge {
    pub(crate) fn borrowed_reader(
        &self,
        session: &mut SessionHost<'_>,
    ) -> Result<BorrowedFormReader, SessionError> {
        self.check(session)?;
        let interrupt = session.host_work_interrupt();
        let (native_kind, field_get, invoke_method) = session.data_live_sequence_calls();
        let lazy = self.core.factories["lazy-sval"].value(session)?;
        session.data_inspect_calls(
            &lazy,
            &lazy,
            |mut store, _, _args_new, _invoke, _closure_new, _, undefined| {
                let undefined = undefined
                    .unwrap_anyref()
                    .map(|value| value.to_owned_rooted(&mut store))
                    .transpose()?;
                Ok(BorrowedFormReader {
                    core: self.core.clone(),
                    invoke_method,
                    native_kind,
                    field_get,
                    undefined,
                    interrupt,
                })
            },
        )
    }
}
impl BorrowedFormReader {
    /// Decode actual data directly, sharing the established nominal decoder,
    /// metadata/sequence rules, cumulative fuel and cancellation flag. No source
    /// printing, reader round trip or evaluation of syntax takes place here.
    pub(crate) fn read(
        &self,
        mut store: StoreContextMut<'_, ()>,
        value: &Val,
        span: Range<usize>,
    ) -> wasmtime::Result<Form> {
        let mut roots = wasmtime::RootScope::new(&mut store);
        let function = self.core.factories["lazy-sval"].val(&mut roots.as_context_mut());
        let undefined = Val::AnyRef(
            self.undefined
                .as_ref()
                .map(|value| value.to_rooted(&mut roots)),
        );
        let mut budget = Budget {
            display: None,
            native: None,
            source_valid: None,
            include_metadata: true,
            interrupt: self.interrupt.clone(),
            active: Vec::new(),
            profile: false,
            host_fuel: std::cell::Cell::new(0),
            lazy_calls: std::cell::Cell::new(0),
            calls: Some(ReadCalls {
                invoke_method: self.invoke_method,
                function,
                native_kind: self.native_kind,
                field_get: self.field_get,
            }),
            undefined: Some(undefined),
        };
        decode(
            &mut roots.as_context_mut(),
            value,
            &self.core.classes,
            &span,
            0,
            &mut budget,
        )
    }
}
