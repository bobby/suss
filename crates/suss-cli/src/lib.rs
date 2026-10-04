//! Native embedding host for the portable compiler and shared runtime.
#[cfg(not(target_family = "wasm"))]
mod portable_module_cache;
#[cfg(not(target_family = "wasm"))]
pub mod portable_macro_data;
#[cfg(not(target_family = "wasm"))]
pub mod portable_macro_graph;
#[cfg(not(target_family = "wasm"))]
pub mod portable_macros;
#[cfg(not(target_family = "wasm"))]
pub mod portable_repl;
#[cfg(not(target_family = "wasm"))]
pub mod portable_session;

#[cfg(not(target_family = "wasm"))]
pub mod portable_aot;
