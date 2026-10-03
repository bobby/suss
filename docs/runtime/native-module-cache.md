# Native compiled module reuse

The native session host reuses Wasmtime `Module` handles after ordinary source
analysis, macro execution and Wasm emission. The process-local cache compares the
complete Wasm bytes and the identity of the engine that compiled them. Changed
constants, imports, ABI sections or generated code cannot hit an earlier entry.
Different engines never share entries, even with apparently identical options.
The process and engine identity fix the compiler/runtime build, native target and
Wasmtime flags for this code cache. It does not deserialize untrusted native code.

Every artifact still passes the runtime ABI check before cache lookup. Every
installation still creates fresh instances and binding cells in its own Store;
initializers, `defonce`, evaluations and compiled macro calls retain their normal
execution rules. The cache owns no instances, globals, Stores or guest roots.
Reset replaces session state. Bounded cached code may remain available afterward.

The cache uses least recently used eviction with limits of 64 module handles and
32 MiB of retained Wasm input bytes. Oversized artifacts compile without entering
the cache. Failed compilation leaves existing entries intact. One mutex prevents
duplicate concurrent compilation. These limits do not measure native JIT memory
or live guest heap. Existing session residency counters concern installed
instances and their artifact sizes; they do not include this process-wide cache.

Focused regressions cover exact input identity, engine separation, eviction,
oversized input and failure preservation, plus actual repeated initialization,
Store isolation, reset and effectful compiled macros with identical output.
The focused/affected run passes 70 tests with no failures or ignores. A temporary
production cache bypass fails the actual reuse assertion; the mutation is fully
restored. Full baseline, independent review and final-head CI remain required.
Commands and logs are retained in the handoff.

This is a code cache prerequisite for M3, not a source/macro analysis cache.
Macro expansion and emission still run on each input. Source-level cache keys
including source, compiler/runtime ABI, macro dependency graph, target and flags,
complete portable schema, evaluator removal and lifecycle acceptance remain open.
No compatibility declaration is promoted and issue #14 remains open.
