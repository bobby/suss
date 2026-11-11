# ADR-0005: Whole-component Binaryen optimization of AOT builds

Status: **Proposed** 2026-11-11.
Issue: [#357](https://github.com/bobby/suss/issues/357) (M15-10).

## Context

An AOT component contains several core modules, which wasmtime compiles with
Cranelift one module at a time:

- the shared runtime;
- the bindings module;
- shipped core;
- one module per top-level form (or per required file);
- the adapter.

Cranelift inlines nothing across Wasm functions, let alone across modules. The
production WasmGC compilers (dart2wasm, Kotlin/Wasm, J2Wasm) all run Binaryen's
`wasm-opt` over a closed world.

Under [ADR-0004](0004-calling-convention.md) each function's code struct is an
immutable global with a constant initializer. In a merged module, Binaryen's
whole-program passes (`--gufa`, devirtualization, inlining) can therefore turn
a `call_ref` of a known closure's entry into a static `call`. That would remove
wasmtime 49's function-reference host call (`docs/upstream/`) without changing
the ABI. Direct linking (#360) already makes calls between user functions
static, but across module boundaries, where nothing can be inlined.

## Measurements so far

Binaryen 133 (Homebrew), `wasm-opt --all-features`, on an Apple M3 Pro.

Shipped core, `runtime/bootstrap/runtime.wasm` (1,920,694 bytes, the M15 branches):

| Level | Size | Time |
| --- | --- | --- |
| `-O1` | 1,182,575 | 0.7 s |
| `-O2` | 1,147,723 | 1.15 s |
| `-O3` | 1,137,202 | 1.44 s |
| `-Os` | 1,139,944 | 1.38 s |

The runtime module goes from 189,952 to 178,640 bytes at `-O2`.

Binaryen accepts both modules: WasmGC, typed function references, tail calls,
exceptions and the shared recursive type group. These are per-module numbers,
without merging or closed-world passes, and not yet checked for behavior or
speed.

## Decision (proposed)

1. **Merge, then optimize.** `suss compile --optimize` merges the component's
   core modules into one with `wasm-merge`, wiring each module's
   `suss.runtime`, `suss.bindings.runtime`, `suss.fragment.N`, `suss.slot.N`
   and `suss.link.N` imports to the matching exports. It then runs
   `wasm-opt --all-features -O2 --closed-world --gufa --type-refining
   --inlining`, and componentizes the result as today. Exports stay the
   component's WIT exports; everything else may be renamed or removed.
2. **Obtaining Binaryen.** `suss compile --optimize` invokes a separately
   installed `wasm-opt`/`wasm-merge`, found on `PATH` or through `SUSS_WASM_OPT`,
   and fails with a clear message if it's missing. A build without
   `--optimize` is unchanged and needs no Binaryen. Vendoring a Wasm build of
   Binaryen, run under wasmtime, is a later option if the flag should work out
   of the box. Binaryen is Apache-2.0, and invoking it adds nothing to Suss's
   shipped code or license.
3. **Optional, off by default.** That holds until the full baseline, including
   the corpus run as components, passes on optimized components.
   `bench.py` and component size are reported with and without it.

## Consequences

- Build time grows by about a second for core, plus merging, per optimized
  build. The build cache should key optimized artifacts separately.
- Optimized components are no longer fragment-per-form, so the incremental
  fragment cache doesn't apply to them. That's acceptable for release builds.
- Stack traces and source names in Wasm function names may change. The
  language's error messages don't depend on them.
- If Binaryen's optimizations rely on assumptions that don't hold for Suss
  (for example, closed-world type refinement while host code creates
  closures), `--closed-world` must be dropped or narrowed. The baseline on
  optimized components decides this.

## Open questions

1. A separately installed tool, or a vendored Wasm build, by default?
2. `-O2` or `-O3` for `--optimize`? The size difference is about 1%.
3. Should `suss run` of a source file ever optimize? This proposal says no.
