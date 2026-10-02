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

The development-only primary corpus contains 35 fresh pinned observations for
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
