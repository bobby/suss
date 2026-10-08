# Compiled hash-map data prerequisites

This work retains the pinned ClojureScript HAMT, node sequences, iterators,
transient array/hash maps and their public mutation/numeric dependencies as
complete licensed source forms. The import manifest binds original bytes,
explicit bootstrap patches and five additional complete factory setup statements
to the pinned upstream commit. Reviews remain `:in-progress`: source retention
and the observations below do not establish complete collection compatibility.

Bootstrap patches preserve all source arities, branches and algorithms. They
expand defn/doto/if-let into the supported bounded forms, map emitted native
member/field spellings consistently in declarations and references, and replace
JavaScript Error construction with the portable exception factory. General
source member munging and reify support are still unfinished. Floor/ceil are
checked binary64 storage adapters; no Java dependency enters the shipped path.

The native macro bridge captures canonical PersistentHashMap, BitmapIndexedNode,
ArrayNode and HashCollisionNode descriptor roots. It traverses actual source
storage in pinned inode order, including the separate nil-key entry first,
without executing arbitrary collection protocols. It checks field widths,
bitmap populations, child counts, collision counts, canonical storage identities
and the declared map count. Signed bit 31 is accepted. The shared 4096-node
budget and bounded hash path reject cycles; class redefinition does not change
previously captured descriptor roots. Existing metadata transport accepts the
resulting canonical map syntax.

The original development-only primary corpus contained 35 fresh pinned observations for
maps of 9/16/17/32/33/65/128 entries, nil keys, replacement/removal, equality/hash,
20 colliding keys, transient growth and closed mutator lifecycle. Native tests
independently inspect numeric bits in both Runtime and Macro Stores. Focused
bridge tests also exercise GC, old canonical roots, matching foreign layouts,
malformed flags/counts/nodes, cycles and nil-entry ordering.

These larger operations currently need a bounded 100,000,000-fuel test allowance.
The default 10,000,000 budget trapped during the 32-entry observation. This is a
performance/resource limitation, not evidence that ordinary frontend operations
already satisfy the full core acceptance gate. Tests preserve the observed
failure rather than treating fuel exhaustion as cancellation.

The source Array.pop adapter mutates the physical source-array owner's storage,
returns the removed value or undefined for an empty array, evaluates then ignores
extra arguments and preserves aliases. Function.call/apply route canonical native method
wrappers through the existing anchored method dispatcher, preserving their
physical receiver; ordinary closures retain their receiver-independent path.
Detached array pop and foreign receivers fail as language exceptions. Raw ABI
regressions additionally inspect forged callback environments and empty buffers.

Remaining work includes canonical NodeSeq/ArrayNodeSeq syntax transport,
complete nil-iter/reify and iterator dependencies, optional undefined arguments
at the strict source/native method boundary, factory error formatting, complete
collection acceptance and performance under the frontend budget. Full &env,
source locations, syntax quote/gensyms, reproducible Java-free compiled bootstrap,
cache invalidation and cooperative cancellation remain M3 requirements. Issues
#12–#15 remain open.

Validation commands and actual terminal results are recorded in
[the handoff](../roadmap/handoff.md). A passing focused suite is not the required
full workspace baseline or final-head CI.


## Bounded deletion and conversion coverage — 2026-10-08

Audit at base `3c80b60` found existing collision update/single deletion (20→19),
map/transient growth, nil entries, equality/hash and retained-root tests. Those
were retained rather than recreated. Missing exact boundaries are now authored:
collision 20→1→0/reinsertion, dense root 32→8→7→0/regrowth, nil/false discrimination
through the eight→nine-entry array-map/hash-map transition, and bidirectional
cross-representation equality/hash (including unequal swapped nil/false values).
Persistent and transient removal paths both have public observations.

`tests/oracle/hash-map-fixture.sus` extends the existing collision fixture with
original development probes. Constant-hash CollisionKey is reused; SlotKey
returns its ordinal as hash to force all 32 root slots independently of host
numeric hash behavior. Removing slots 0..24 leaves only high slots 25..31: losing
high children during packing must fail individual lookups and seq/count checks.
Metadata and earlier persistent maps remain observable after deletion.

All original 35 cases/expressions/expectations remain byte-equivalent as parsed
JSON. The corpus adds 76 scalar observations for 111 total. Fresh **pinned
ClojureScript/Node** execution matches all 111 exact f64-bit expectations, with
Boolean results deliberately converted to 0/1 for the existing independent native
number decoder. No reference/native divergence or skip was introduced. This is
reference evidence: the expanded Rust suite has not been compiled or executed.

A focused native regression is authored in `compiled_macro_hash_maps.rs` for
Runtime and Macro phases, with the existing finite 100M test fuel allowance and
GC. Private probes complement the shared public corpus: collision child count
20→1 and empty root; ArrayNode counts 32→8, BitmapIndexedNode packing with 14-slot
array and signed bitmap -33554432, then dense regrowth; and actual PersistentArrayMap
versus PersistentHashMap identities before/after conversion. A separate pinned
shape probe confirms these structural assumptions, not native execution.

No production source or upstream algorithm changed. Generated core/license
artifacts are untouched: 323 import files reproduce; the existing review overlay
still validates 420 reviewed/645 unassessed. Custom keys/fixtures/tests are original
MIT/Apache-2.0 development code, not copied ClojureScript implementation. Existing
EPL provenance remains attached to the retained HAMT forms.

The lane is `/private/tmp/suss-m4-hamt-deletion`, branch
`portable/m4-hamt-deletion` based on `3c80b60`. Python strict corpus/transport tests
pass 5+8; touched Rust parses through rustfmt; diff whitespace check passes. No
Cargo/native build, commit or push ran here while baseline PID 65986 owns the native lane.
Exact commands, oracle setup failures, logs and remaining gates are in the handoff.
Next run the complete `compiled_macro_hash_maps` native suite after scheduling,
resolve genuine compile/semantic/resource failures, then require full workspace
baseline, independent final review and exact-head CI. This bounded coverage does
not establish sorted collections, records or complete #18/#19 acceptance.
