# Resurrection roadmap

The accepted target is [the design specification](docs/design/suss-0.3.1.md).
The prototype has not reached the WASI alpha gate. Work below is dependency ordered;
status is evidence-based and is not a calendar promise. See the [handoff](docs/roadmap/handoff.md).

[ADR-0001](docs/adr/0001-result-option-and-panic.md) proposes separate tactical and
strategic adoption of Result/Option/panic semantics. Its
[dedicated milestone and work packages](docs/adr/README.md#adr-0001-work-tracking)
track decisions, design, implementation, and acceptance separately from M0–M9.
The proposal has not changed this roadmap's accepted semantics or release gates.

| Milestone | Depends on | Exit gate |
| --- | --- | --- |
| M0: Contract and feasibility | — | All M0 acceptance criteria pass |
| M1: Trustworthy evidence | M0 | All M1 acceptance criteria pass |
| M2: Compiler and runtime foundation | M0, M1 | All M2 acceptance criteria pass |
| M3: Persistent development environment | M2 | All M3 acceptance criteria pass |
| M4: Portable persistent collections | M2 | All M4 acceptance criteria pass |
| M5: Generic WIT interoperability | M2, M4 | All M5 acceptance criteria pass |
| M6: WASI 0.3.1 alpha | M3, M4, M5 | All M6 acceptance criteria pass |
| M7: Portable compatibility beta | M6 | All M7 acceptance criteria pass |
| M8: Browser beta | M6 | All M8 acceptance criteria pass |
| M9: CSP extension | M6, M7 | All M9 acceptance criteria pass |

M4 and M5 can overlap after their foundations; M8 feasibility happens in M0,
with browser product delivery after the WASI alpha. CSP is deliberately last.

## Work packages

[Machine-readable issues](docs/roadmap/issues.json) contain stable IDs, dependencies,
objectives, acceptance criteria, scope and proposed validation commands. Commands
for planned suites are **acceptance targets**, not existing runnable scripts.
Published: [10 milestones](https://github.com/bobby/suss/milestones) and
[39 issues](https://github.com/bobby/suss/issues). Stable IDs map to remote URLs in
[github.json](docs/roadmap/github.json). The publisher creates missing entries and
preserves existing issue bodies.

### M0: Contract and feasibility

- **M0-01 — Contract and upstream inventory** (completed). Deterministic pinned inventory, source hashes/reader branches, strict review schema and provenance policy pass. 206 source declarations now have in-progress manual reviews; 859 remain unassessed for M4/M7 implementation.
- **M0-02 — Lock toolchain and official WIT packages** (completed). Locked official package hashes and executing GC/tail-call/EH/map/implements/external-id/async/future/stream probes pass. Generated Suss adapters remain M5/M6 work.
- **M0-03 — Prove shared GC fragments** (completed). Shared roots/closures/nominal descriptors survive forced GC; incompatible ABI fails before initialization. Production persistent sessions remain M2/M3 work.
- **M0-04 — Prove browser loading and suspension** (completed). Chrome executes typed core/Promise/cancellation/feature-error fixtures and optional Jco GC packaging. Teardown timeout is recorded separately; cross-browser/product delivery remains M8 work.

### M1: Trustworthy evidence

- **M1-01 — Strict conformance decoding and baseline** (completed). Executing negative regressions reject wrong/malformed/unknown results, missing cases, changed failures and unexpected passes; 201 reviewed legacy cases pass.
- **M1-02 — ClojureScript oracle and semantic regressions** (completed). Fresh pinned Node and independently decoded Suss artifacts compare one corpus covering ordered effects, binary64 bits, UTF-16, arities and exceptions: 9 differential passes, 7 exact failures, 0 skips. This completes the evidence harness; repairing semantic failures and broader compatibility remain M2/M4/M7 work.
- **M1-03 — Bounded CI and reproducible baseline** (completed). Lockfiles, shared engines, bounded fuel/traversal, two workers and an initial 25-minute CI budget are exercised by successful reviewed-head and merged-main full baselines. Ignored/manual tests remain explicit.

The [acceptance audit](docs/roadmap/acceptance-m0-m1.md) maps every M0/M1 criterion
to merged implementation, executing evidence and its scope. Reconciliation PR #43
merged with explicit closing links; GitHub issues #1–#7 and milestones M0/M1 are
closed after their exit gates were rechecked. M2 foundation was accepted through
merged PR #115/#116; issues #8–#11 and milestone M2 are now closed. M3–M9 remain open.

### M2: Compiler and runtime foundation

- **M2-01 — Reader forms, metadata and namespace phases** (completed foundation). Located forms/metadata, deterministic source ambiguity and feature ordering, separate executed Runtime/Macro phase imports and canonical cljs.core aliases satisfy issue #8. Source compiled macro loading remains M3.
- **M2-02 — Explicit evaluation-order IR** (completed foundation). Executed calls, conditions, collection entry interfaces, dispatch and recur preserve once-only order; arity/type diagnostics and legacy receiver/bitwise/hash operand repairs satisfy issue #9. Literal constructor fixtures establish evaluation behavior; persistent collection implementations remain M4.
- **M2-03 — Runtime ABI v1 and closures** (completed foundation, now ABI2). Shared rooted values, universal invocation/arity, binary64/UTF-16 and pre-execution ABI rejection satisfy issue #10. The accepted ABI2 identity extension requires rebuilt artifacts.
- **M2-04 — Nominal types, protocols and exceptions** (completed foundation). Same-layout nominal identity, cross-fragment protocol extensions, try/catch/finally, dynamic bindings and unknown-type diagnostics satisfy issue #11. Complete core/error/printing surfaces remain later compatibility work.

The [criterion-by-criterion audit](docs/roadmap/acceptance-m2.md) maps all21 published
foundation criteria to executing evidence. User-merged #115/#116 place the reviewed
implementation on main. #116 reviewed-head CI36912150217 passed; merged main7a9010b
has the identical entire file tree. Required full baseline passed843/0/17existing
ignores; the named focused acceptance passed529/0/12existing ignores. This does not
claim M3 compiled macros/frontends, M4 persistent collections, runtime metadata or
any release gate complete.

### M3: Persistent development environment

- **M3-01 — Incremental compiled REPL** (in-progress). Replace source replay with one runtime and compiled input fragments. The native command uses one persistent compiled session. Eight command tests and four atom tests establish persistence/reset prerequisites; six new command/host macro regressions integrate standalone compiled definitions, source reload and two-phase reset ([macro prompt](docs/runtime/compiled-repl-macros.md)); complete frontend/core acceptance remains; see [session host](docs/runtime/portable-session.md).
- **M3-02 — Namespace loading and redefinition** (in-progress). Implement live binding cells, namespace loading, defonce and reload semantics. Source graph preparation and eleven executing module tests now establish compiler prerequisites; The native command now supplies load/reload/reload-all/in-ns, with four executing namespace tests; source reload metadata, compiled macro/cache/privacy policy remain; see [module preparation](docs/runtime/portable-modules.md).
- **M3-03 — Compiled macro bootstrap** (in-progress). Run macros in a separate compiled phase session and remove the temporary evaluator. Native phase sessions now execute retained core, globals and source dependencies in separate Stores with phase-qualified cells and reset; see [phase execution](docs/runtime/compiled-phase-session.md). Reader forms and actual GC data now cross the same compiled macro/runtime pipeline without source printing/rereading; see [macro form transport](docs/runtime/compiled-macro-forms.md). Explicitly registered source macro functions now execute in the isolated Store and expand in lexical HIR analysis, including source dependencies; see [source macro execution](docs/runtime/compiled-source-macros.md). Source definitions now support multiple signatures, docs/source attributes and macro-body expansion in the same phase Store. Explicit source macro imports now execute dependencies in the isolated Store with aliases/refers/renames; see [phase imports](docs/runtime/compiled-macro-imports.md). Macro libspec reload/reload-all now refreshes reachable phase source while preserving old Runtime expansions; see [macro reload](docs/runtime/compiled-macro-reload.md). Persistent vector macro data now uses complete retained source types/trie/transient algorithms, quoted large-vector factory construction and canonical GC transport; 40 fresh primary observations match both native phases ([vector data](docs/runtime/compiled-macro-vectors.md)). Persistent array-map macro data now executes retained types with canonical GC transport; 24 fresh primary observations match both native phases ([map data](docs/runtime/compiled-macro-maps.md)). Retained HAMT/transient source and canonical hash-map transport now have focused executing evidence, including 35 pinned observations in both phases; full baseline, review and CI are pending ([hash-map prerequisites](docs/runtime/compiled-macro-hash-maps.md)). Canonical HAMT sequence cursors now have 27 fresh exact pinned observations in both phases, with retained collection conversion/completion dependencies ([sequence transport](docs/runtime/compiled-macro-hamt-sequences.md)); full validation remains pending. Complete ordinary reload/privacy/cache policy and bootstrap remain unfinished. Six bounded checked control-macro expansions now establish retained-source prerequisites; this is not compiled macro acceptance. See [control macros](docs/runtime/control-flow.md). Forward declarations now preserve source undefined reads and defonce initialization; see [declarations](docs/runtime/forward-declarations.md). The bounded caching-hash dependency now has38 fresh scalar observations and located recovery guards; full compiled macros and List hashing remain open; see [hash caching](docs/runtime/caching-hash.md).
  Compiler analysis now retains actual lexical, namespace and source-position facts as prerequisites for rich `&env`; eight pinned position observations and seven focused native tests have executing evidence. Source macros do not yet receive that environment. See [environment facts](docs/runtime/compiled-macro-environment-facts.md).
  Explicit analyzer contexts and actual field/shadow records now have focused executing evidence; 24 pinned ordered context observations and 15 result strings agree in both Stores. Runtime and compile-phase try ordering are tested separately. Full child validation and rich source-level `&env` remain open; see [context and field facts](docs/runtime/compiled-macro-analysis-context.md).
  Actual named function scopes now preserve source declarations, phase namespaces, parent scopes and real self/shadow bindings; definition hints introduce no lexical binding ID. Focused native execution and independent review establish this prerequisite. Rich source-level `&env` and original M3 acceptance remain open; see [function scope facts](docs/runtime/compiled-macro-function-scopes.md).
  Method binding facts now distinguish physical IDs from source receiver/argument roles, retaining actual type declarations and original protocol argument shadows. Six pinned observations and four executed results agree in both Stores; rich `&env` remains open. See [method binding roles](docs/runtime/compiled-macro-method-roles.md).
  Actual reader/expansion syntax now accompanies analyzed HIR, with explicit absence on compiler-only nodes. Focused executing evidence passes; full source AST/inference and canonical rich `&env` transport remain unfinished ([source analysis records](docs/runtime/compiled-macro-source-analysis.md)).
- **M3-04 — Session lifecycle and interruption** (in-progress). Define reset, roots, code residency and cancellation while interactive I/O is pending. Native session reset/owned handles/fuel recovery/residency counters now have executing evidence; interactive cancellation and live heap accounting remain.

Retained cached string hashing now matches64 fresh pinned observations over owned GC
objects, with aliases/live dependencies and40 ABI checks. Public hash/equality and
persistent collections remain unfinished; see [cached hashing](docs/runtime/cached-string-hashing.md).

### M4: Portable persistent collections

- **M4-01 — Upstream extraction and adaptation provenance** (in-progress). Reviewed ID selection now reproduces exact source forms, explicit hash-bound patches, EPL packaging and a generated canonical core artifact. Seven bootstrap identity/not/boolean/some?/inc/dec/fn? forms and twenty-one retained Fn/sequence/collection protocol declarations execute with GC/redefinition/order regressions and separate 117-case function /65-case interface primary corpora. Retained bit-count and int-rotate-left add two source algorithms with explicit bootstrap patches; scalar bitwise/imul and bounded macros have70 fresh primary/native observations. Retained scalar Murmur algorithms/constants and zero? now have57 exact observations and ten additional source forms, supported by bounded threading/zero expansion. Full core dependencies/macros/loading acceptance remains; see [core import](docs/compatibility/CORE-IMPORT.md).
- **M4-02 — Sequences, lists and vectors** (in-progress). Direct-only protocol implements? now has33 fresh primary/native observations and phase/GC/namespace guards; see [implementation predicate](docs/runtime/implements.md). GC-owned mutable array storage, scoped nominal field mutation and bounded source macro/runtime adaptations now establish prerequisites for IndexedSeq, list and variadic rest; see [array foundations](docs/runtime/arrays.md). UTF-16 alength/aget now provide string storage access required by retained IndexedSeq; see [indexed strings](docs/runtime/indexed-strings.md). Persistent sequence/list/vector/subvector/map-entry and lazy/chunked acceptance remain unfinished.
- **M4-03 — Maps, sets, queues, records and sorted types** (planned). Port HAMTs, sorted collections, queues and record behavior.
- **M4-04 — Hashing, metadata, transients and reduction** (planned). Complete shared collection protocols and all reduction paths.

### M5: Generic WIT interoperability

- **M5-01 — Generate bindings from resolved WIT** (in-progress). Replace hardcoded WASI names with selected-world binding generation.
- **M5-02 — Resource ownership and scopes** (planned). Implement constructors, methods, statics, own/borrow and explicit close.
- **M5-03 — Canonical memory allocation and cleanup** (planned). Implement checked realloc/free, post-return and async transfer lifetimes.
- **M5-04 — Bidirectional Rust interoperability fixtures** (planned). Generate an independent Rust host/guest corpus for all WIT boundary shapes.

### M6: WASI 0.3.1 alpha

- **M6-01 — Continuation scheduler and future API** (planned). Implement future/await with GC continuation state machines and a cooperative scheduler.
- **M6-02 — Canonical async, futures and streams** (planned). Connect continuations to canonical async imports/exports and future/stream values.
- **M6-03 — Complete official WASI capability bindings** (planned). Generate and exercise the entire pinned WASI 0.3.1 package graph.
- **M6-04 — CLI commands, HTTP applications and interactive I/O** (planned). Deliver command/library workflows and alpha release gate.

### M7: Portable compatibility beta

- **M7-01 — Finish portable public core and macros** (planned). Review and implement every remaining portable inventory item.
- **M7-02 — Sequence and transducer behavior** (planned). Complete higher-order arities, transducers, chunking and laziness effects.
- **M7-03 — State, delays, multimethods and printing** (planned). Complete atoms/watches/validators/CAS, volatiles, delays, multimethods and printer contracts.
- **M7-04 — Compatibility and migration release report** (planned). Publish evidence and retire superseded prototype paths.

### M8: Browser beta

- **M8-01 — ES module and declaration packaging** (planned). Produce distributable browser modules using the validated M0 path.
- **M8-02 — DOM, fetch, events and Promise bridge** (planned). Implement typed browser host APIs and shared async semantics.
- **M8-03 — Browser application and library examples** (planned). Ship examples demonstrating both browser usage modes.
- **M8-04 — Cross-browser corpus and distribution** (planned). Run the shared compatibility corpus in declared browser versions.

### M9: CSP extension

- **M9-01 — Channels, buffers and selection** (planned). Implement CSP channels over the established scheduler.
- **M9-02 — go state machines** (planned). Implement core.async-style go lowering using continuation infrastructure.
- **M9-03 — Future and stream channel adapters** (planned). Provide explicit adapters between CSP and component async values.
- **M9-04 — core.async compatibility corpus** (planned). Port selected core.async tests with documented scope.

## Completion discipline

Every implementation session adds a failing regression, makes the smallest coherent
change, executes its relevant checks, and updates evidence/handoff. A package
is complete only when its acceptance criteria pass. A known-failure baseline
does not certify compatibility. Toolchain limitations remain explicit blockers,
not reasons to silently weaken the contract.


Sequence/list foundation progress: retained source List/EmptyList/Cons/IndexedSeq,
canonical empty literals and persistent variadic rest now execute against75 fresh
primary observations. All original51 are preserved. Imported68/72 artifacts and
157 partial/908 unassessed reviews remain prerequisites, not M4 acceptance; full
method dependencies and surrounding release gates remain incomplete. See
[sequence evidence](docs/runtime/sequences.md).

Retained sequential equality now closes the List/EmptyList/Cons/IndexedSeq helper
boundary with source `=`/equiv-sequential and native number/default IEquiv setup.
Fresh131 observations retain the prior75;159 partial reviews/906 unassessed and
70 selections/74 licensed artifacts remain prerequisites. Hashing/reduction and
remaining collection/macro/release gates stay open; see
[sequential equality](docs/runtime/sequential-equality.md).

Retained reduction now supplies IDeref/Reduced and source sequence/array/string/
iterator helpers, with65 fresh primary/native observations and81 control cases
including bounded if-let. Selection82/artifacts86 and172 partial/893 unassessed
reviews remain prerequisites; complete collection/transducer/macro/release gates
stay open. See [reduction evidence](docs/runtime/sequence-reduction.md).

Retained IndexedSeqIterator/RSeq and reversible?/rseq now execute against59 shared
fresh primary/native observations plus1 explicit named length-write boundary.
Selection86/artifacts90/reviews176partial+889unassessed remain prerequisites;
hash/printing/index helpers, generic reverse, other collections and compiled
macro/release gates remain open. See [iteration evidence](docs/runtime/sequence-iteration.md).

Identity hashing prerequisites now have34 fresh primary observations,33 matching
native relations and1 exact strict-arity contract boundary across4 native tests. Owner-held UID storage advances the shared layout to ABI2 with explicit
old-artifact rejection; complete retained IFn/MetaFn/with-meta/meta dependencies
bring selection to96 forms/100 licensed artifacts and189 partial reviews/876
unassessed. Public/default and collection hashing, general IFn invocation, apply,
full metadata and milestone gates remain open. See
[identity evidence](docs/runtime/identity-hashing.md).


Scalar `case` bootstrap now supports grouped binary64/UTF-16 literals and bounded
boolean/nil equality tables with selector-once evaluation and live qualified
core equality. There are 40 fresh pinned/native value matches and one separately
asserted pinned empty-group parse failure; native rejects that input with a
located compile diagnostic. Actual fragments execute in Runtime and Macro
phases. Selection97/artifacts101 and192 partial/873 unassessed reviews remain
prerequisites. Full compiled macros, complete case/case*, public hash Date
handling, collections and release gates remain unfinished. See
[scalar case evidence](docs/runtime/scalar-case.md).


Retained public scalar hash now preserves all pinned source branches, including
an explicit descriptor-backed numeric Date storage adaptation. Fresh42 pinned/
native observations and4 native tests cover scalar bits, UTF16 composition,
protocol priority, default identity and Date normalization/GC. Selection98/
artifacts102 and193 partial/872 unassessed reviews remain prerequisites. Full
Date/Inst/reader/printing, collection composition, compiled macros and release
gates remain open; see [public scalar hash evidence](docs/runtime/public-scalar-hash.md).


Retained ordered/unordered collection hash helpers and the empty unordered hash
initializer now execute through the retained source pipeline. Fresh55 pinned/
native observations and5 native tests cover sequential hash agreement, nested
values, UTF16, duplicates/count, caches/metadata, effects and GC recovery. Core
selection101/artifacts105 and196 partial/869 unassessed reviews remain
prerequisites. Persistent vectors/maps/sets/map entries, collision nodes and
compiled macro/release gates remain required; see
[collection hashing evidence](docs/runtime/collection-hashing.md).


Vector trie prerequisites retain the complete pinned VectorNode and nine private
node/path/update helpers. Selection111/artifacts115 and206 partial/859 unassessed
reviews do not establish full PersistentVector or M4 acceptance. Fresh23 exact
oracle values and four native tests cover shallow ownership, structural sharing,
GC, tail boundaries, recursive association/removal and exception recovery.
See [vector trie evidence](docs/runtime/vector-trie.md).


M2-02 collection expression lowering now has19 fresh pinned/native observations:
15 shared values and4 separately asserted observations of the accepted textual
map/set evaluation-order variance,0 skips. Five native tests guard constructor
interfaces, method capture, thrown-entry order, GC and located missing-class
diagnostics. This is a compiler prerequisite using development-only fixtures;
full collection/core and M2 acceptance remain open. See
[collection literals](docs/runtime/collection-literals.md).


[M2 acceptance candidate](docs/roadmap/acceptance-m2.md) maps every published
foundation criterion to actual source and executing guards. The runnable focused
command is `sh scripts/test-m2-foundation.sh`; future compiled macros, complete
collections and production frontend migration remain separate gates. M2 statuses
stay in progress pending independent criterion review and final-head CI.

M3-01 native command frontend now evaluates each complete input in the persistent
Session instead of accumulating/replaying source. Executing command tests cover
once-only initializers/defonce, old function captures, compile/runtime recovery,
multiline input, exact scalar display and reset. Atoms, namespace command/reload
policy, compiled macro bootstrap, interruption and live heap acceptance remain
open; see [compiled REPL frontend](docs/runtime/compiled-repl.md).
