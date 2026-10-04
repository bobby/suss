# Core binding initialization artifacts

The portable compiler's `core_bindings` module emits the canonical core cells and
initialization calls as a shared-ABI Wasm module. Native Session creation now uses
this artifact so AOT target assembly can use the same initialization. Focused
execution passes; AOT commands and evaluator retirement remain incomplete.

The module imports factory functions only from `suss.runtime`, exports cells by
phase-qualified language identity, and initializes them in its start function.
Runtime factory imports are deduplicated by export name so the module can be
embedded in a component. Distinct language cells retain separate initialization
calls and closure values even when they use the same factory.
The ExceptionInfo class is initialized before factories capture its binding cell.
The ex-info and four variadic bitwise factories receive their own bound nil cell
before the constructed closure is installed. Ordinary source initialization still
runs through explicit fragment `eval` calls after host validation and linking.

`compile_with_cells` includes source declaration cells discovered by portable
preparation. These remain unbound until source fragments initialize them. Repeated
requests deduplicate the same identity; foreign-phase cells produce a diagnostic.
The generated module carries the compiler/ABI/phase identity and body digest.
Its known empty macro graph describes this compiler-generated initialization,
while source provenance remains unknown; user source fragments carry their own
available provenance. It does not reconstruct user catalogs from raw Wasm.

Native installation checks the generated identity and validates/links before its
start function executes. Session statistics separately expose base runtime plus
initializer input sizes and user fragment input sizes. These are artifact byte
counts, not JIT allocation sizes or live-object counters. Reset rebuilds the
base runtime/cells and discards the old Store and user fragments.

The direct executing test performs instantiation, bound-flag checks, forced GC,
all four self-cell bitwise operations, arithmetic closures and descriptor-backed
ExceptionInfo in both phases. A second test installs source cells and two actual
fragments, preserving an incrementing closure without replaying its initializer;
a third rejects foreign-phase cells. The first component embedding regression failed on duplicate imports for the two
unsigned-shift cells; the repaired module now embeds and instantiates in both
phases. All four compiler tests pass, including independently executing both
shift aliases and preserving their distinct closures. Seventy-two
affected native bootstrap, session, namespace, phase, command, bitwise and
ExceptionInfo tests pass, plus seven native lifecycle guards. Both phase Wasm/JSON
assets reproduce byte-for-byte without Java; four bootstrap tests and Python118
pass. Independent review, the unfiltered full workspace baseline and exact
final-head CI remain required.

This code is original Rust. It ports no upstream forms and changes no shared GC
layout. Selected-WIT adapters, component/AOT command migration, legacy evaluator
retirement, published dependency-loader policy and the original scheduler/lifecycle
criteria remain open M3 work.
