//! One Store authority: owned by a session or temporarily borrowed from a Caller.
//! No reference to the owning Session or second mutable Store is retained.
use wasmtime::{AsContext, AsContextMut, GcHeapOutOfMemory, Store, StoreContext, StoreContextMut};

pub(super) enum StoreAccess<'a> {
    Owned(Store<()>),
    Borrowed(StoreContextMut<'a, ()>),
}
impl AsContext for StoreAccess<'_> {
    type Data = ();
    fn as_context(&self) -> StoreContext<'_, ()> {
        match self {
            Self::Owned(store) => store.as_context(),
            Self::Borrowed(store) => store.as_context(),
        }
    }
}
impl AsContextMut for StoreAccess<'_> {
    fn as_context_mut(&mut self) -> StoreContextMut<'_, ()> {
        match self {
            Self::Owned(store) => store.as_context_mut(),
            Self::Borrowed(store) => store.as_context_mut(),
        }
    }
}
impl StoreAccess<'_> {
    pub(super) fn is_borrowed(&self) -> bool {
        matches!(self, Self::Borrowed(_))
    }
    pub(super) fn owned(&self) -> Option<&Store<()>> {
        match self {
            Self::Owned(store) => Some(store),
            Self::Borrowed(_) => None,
        }
    }
    pub(super) fn get_fuel(&self) -> wasmtime::Result<u64> {
        self.as_context().get_fuel()
    }
    pub(super) fn set_fuel(&mut self, fuel: u64) -> wasmtime::Result<()> {
        self.as_context_mut().set_fuel(fuel)
    }
    pub(super) fn set_epoch_deadline(&mut self, deadline: u64) {
        self.as_context_mut().set_epoch_deadline(deadline);
    }
    pub(super) fn has_pending_exception(&self) -> bool {
        match self {
            Self::Owned(store) => store.has_pending_exception(),
            Self::Borrowed(store) => store.has_pending_exception(),
        }
    }
    pub(super) fn gc(&mut self, why: Option<&GcHeapOutOfMemory<()>>) -> wasmtime::Result<()> {
        self.as_context_mut().gc(why)
    }
}
