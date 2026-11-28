//! Original Rust oracle fixture with canonical bindings from pinned wit-bindgen.
use std::sync::atomic::{AtomicU32, Ordering};
wit_bindgen::generate!({ path: "api.wit", world: "rust-provider" });
use exports::fixture::rust_interop::provider::{Guest, Packet};
static CALLS: AtomicU32 = AtomicU32::new(0);
struct Oracle;
impl Guest for Oracle {
    fn echo(value: Packet) -> Packet {
        CALLS.fetch_add(1, Ordering::Relaxed);
        value
    }
    fn calls() -> u32 {
        CALLS.load(Ordering::Relaxed)
    }
}
export!(Oracle);
