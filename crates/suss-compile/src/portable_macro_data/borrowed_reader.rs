use super::*;

/// Captured nominal form-data authority for a callback attached to the same
/// Session Store. This crate-private handle never accepts cross-Store values;
/// capture checks the bridge's existing owner before installing a callback.
/// The callback's surrounding evaluation scope owns dynamic-frame restoration.
pub(crate) struct BorrowedFormReader {
    core: Arc<BridgeCore>,
    args_new: wasmtime::Func,
    invoke: wasmtime::Func,
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
        let lazy = &self.core.factories["lazy-sval"];
        session.data_inspect_calls(
            lazy,
            lazy,
            |mut store, _, args_new, invoke, _, _, undefined| {
                let undefined = undefined
                    .unwrap_anyref()
                    .map(|value| value.to_owned_rooted(&mut store))
                    .transpose()?;
                Ok(BorrowedFormReader {
                    core: self.core.clone(),
                    args_new,
                    invoke,
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
            source_valid: None,
            include_metadata: true,
            interrupt: self.interrupt.clone(),
            active: Vec::new(),
            profile: false,
            host_fuel: std::cell::Cell::new(0),
            lazy_calls: std::cell::Cell::new(0),
            calls: Some(ReadCalls {
                args_new: self.args_new,
                invoke: self.invoke,
                function,
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
