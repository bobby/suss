//! Native frontend exports for the shared compiler host.
//!
//! Retain embedding paths while host ownership lives with the compiler pipeline.
#[cfg(not(target_family = "wasm"))]
pub use suss_compile::{
    portable_aot, portable_macro_data, portable_macro_graph, portable_macros,
    portable_project, portable_repl, portable_session,
};
