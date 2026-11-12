# ADR-0005: Whole-component Binaryen optimization of AOT builds

Status: **Accepted** 2026-11-11.
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

## Measurements of merged components

A component exporting loops over `(-equiv i 7)`, `(count "abcdefgh")` and
`(nth v i)` on a 1024-element vector, built by #375's head (06783f4d). The
counts are user-space instructions per iteration from macOS `time -l`
(arm64), with a run of n = 1M less a run of n = 0. All variants return the
same results.

| Case | today | merged | merged, `-O3` | merged, closed-world `-O3` |
| --- | --- | --- | --- | --- |
| `(-equiv i 7)` | 1,746 | 1,744 | 1,689 | 1,674 |
| `(count "abcdefgh")` | 1,896 | 1,890 | 1,599 | 1,594 |
| `(nth v i)` | 3,971 | 3,897 | 3,406 | 3,392 |

- **Merging alone** changes little.
- **`-O3`** on the merged module takes 14–16% off `nth` and `count`.
- **Closed-world passes** (`--type-ssa --gufa --type-refining`) add under 1%,
  for about 2 s more.
- **Size:** the merged module is 2.09 MB, and 1.49 MB at `-O3`.
- **Startup** falls from 0.17 s to 0.12 s.

Binaryen can't reach the calls that cost most:
- calls through mutable var cells;
- protocol dispatch through runtime tables;
- wasmtime 49's host calls wherever a `call_ref` or a `$closure` cast remains.

That remaining cost is #378's work.

## Decision

1. **Merge, then optimize.** `suss compile --optimize` (`-O`) merges the
   component's runtime, bindings, fragment and adapter modules with
   `wasm-merge`, in instantiation order, so their start functions run in the
   same order.
   - Each fragment's direct-link imports (`suss.link.N`, `suss.slot.N`) are
     renamed to the defining fragment's `suss.fragment.N` first, as a merged
     module has one name.
   - The merged module exports only the adapter's exports, and imports only
     the allocator, if the component has one.
   - `wasm-opt -O3` optimizes it, and the component is assembled around it as
     before (`portable/aot/optimize.rs`).
2. **No closed-world passes.** They gained under 1%. Whole-program type
   refinement also assumes no closure is made outside the module, which the
   host breaks. #379 can add them as an option.
3. **A separately installed Binaryen.**
   - `--optimize` runs `wasm-opt` from `SUSS_WASM_OPT` or `PATH`, with
     `wasm-merge` beside it.
   - It requires version 133 or later, and fails with a clear message
     otherwise.
   - A build without `--optimize` is unchanged and needs no Binaryen.
   - Vendoring a Wasm build of Binaryen is a later option, if `--optimize`
     becomes a default.
   - Binaryen is Apache-2.0, and running it adds nothing to Suss's shipped
     code or license.
4. **wasmtime 49's features only.**
   - Binaryen runs with `--all-features` minus every proposal wasmtime 49
     lacks: custom descriptors, compact imports, strings, stack switching,
     shared-everything, fp16, multibyte, custom page sizes, wide arithmetic,
     and the newer atomics.
   - With custom descriptors it narrows exported globals to exact reference
     types, which break instantiation.
   - With compact imports it writes an import encoding that wasmtime 49 can't
     parse.
5. **Scope.**
   - `--optimize` covers WIT-export components: file, namespace and project
     modes. It isn't yet supported for `--main` command components or for
     async imports, and says so.
   - `suss run` of a source file doesn't optimize. Pre-optimizing from other
     subcommands, cached separately, is #379.
   - Optimized builds are cached under their own key.
6. **Off by default**, until the full baseline, including the corpus run as
   components, passes on optimized components.

## Consequences

- **Build time:** an optimized build takes several seconds longer, mostly in
  `wasm-opt` over core: about 5 s for a small component on an M3 Pro.
- **Fragment cache:** optimized components are no longer fragment-per-form, so
  the incremental fragment cache doesn't apply to them. That's acceptable for
  release builds.
- **Names:** stack traces and source names in Wasm function names may change.
  The language's error messages don't depend on them.
- **The old flag:** the previous `--optimize`, which ran `wasm-opt` over the
  finished component, never worked: Binaryen doesn't read components. It is
  replaced.
