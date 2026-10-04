# Portable AOT component assembly

`portable::aot::component` assembles already prepared Runtime fragments into an
executable component using the same shared runtime and compiled core/source
binding initializer as native sessions. The caller supplies a resolved WIT world
and explicit export-to-source-var mappings. This development increment supports
pure freestanding bool/f32/f64 functions and void results. Other WIT boundary
shapes return explicit unimplemented diagnostics. Existing CLI compile commands
still use the legacy pipeline; this API does not complete their migration.

The component links every fragment before a private adapter start function calls
their initializers once in supplied source order. Export wrappers load live cells
and invoke ordinary compiled closures. Multiple WIT exports may map the same
source var; their wrappers share one imported cell. Core module import names must
be unique when embedded in components, even where standalone validation would
accept duplicates. Component validation is mandatory before returning bytes.
Each source artifact is checked against the current compiler/ABI and Runtime
phase before assembly. This API requires prepared compiler catalogs; it does not
reconstruct catalogs or validate a complete dependency policy from raw Wasm.

Bool inputs map to the language's false/true values. Numeric inputs box binary64
values, promoting f32 first. Outputs require actual booleans or boxed numbers;
f32 output demotes the binary64 value. Incompatible values throw the shared
language exception with a boundary-specific message rather than a raw cast trap.
Pure worlds gain no hidden host imports. Void results discard the source value.

The executing compiler suite passes five tests. Typed component calls cover
initializer order, old function captures versus redefined live cells, independent
fresh instances, forced GC, binary64 edge bits, f32 conversion, boolean and void
results, missing/duplicate/unknown mappings, unsupported boundaries and Macro
artifact rejection. Wrong-result cases independently decode language payloads
for invalid boolean sentinels/non-i31 values and invalid numeric results.
The shared-var regression failed against the prior adapter with a duplicate
`user/shared` import; after import sharing, both exports execute successfully.
The compiled-macro CLI fixture passes: a macro in an isolated compiled phase
expands source against the shipped core catalog, and the emitted component
executes its initializer and calls without host Runtime replay, including GC.
The fixture originally had a redundant manifest unwrap and inherited the core
namespace while defining its macro in user; correcting the helper and making
(ns user) explicit preserved every effect and payload assertion. Both refreshed
bootstrap images reproduce byte-for-byte without Java, bootstrap4 and Python118
pass. Independent review, full baseline and final-head CI remain required.

This is original Rust and ports no upstream forms. Shared GC layout and ABI2
remain unchanged. Complete selected-WIT adapters and published artifact policy,
AOT frontend migration, evaluator retirement and scheduler/cancellation/live heap
acceptance remain open. These focused results do not certify M3 completion.
