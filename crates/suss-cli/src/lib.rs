//! Native embedding host for the portable compiler and shared runtime.
#[cfg(not(target_family = "wasm"))]
pub mod portable_session;
