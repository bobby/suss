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
closed after their exit gates were rechecked. M2–M9 remain open.

### M2: Compiler and runtime foundation

- **M2-01 — Reader forms, metadata and namespace phases** (in-progress). Portable reader forms retain byte spans, metadata, binary64/UTF-16 and source-ordered conditionals; 14 scalar observations match the pinned reader and execute through ABI intrinsics. Explicit phase namespace environments now resolve aliases/refers/exclusions and shared live cells; leading source ns directives and def/defonce now compile against supplied declarations; immutable recursive source graphs now compile in require order with located dependency errors; production loading and compiled macro integration remain; see [reader forms](docs/runtime/reader-forms.md).
- **M2-02 — Explicit evaluation-order IR** (in-progress). Replacement HIR/IR now emits validated shared-ABI fragments for scalars, lexical let/do/if and primitive arithmetic with checked dynamic coercions. Sixteen executing tests cover unary arithmetic identity over dynamic values, source order, short circuiting, binding identity, dominance and parallel edge replacement. Eleven additional namespace/cell tests cover ordered global reads, phase isolation, cell updates and source ambiguity. Thirteen source-closure tests cover universal computed/local/global calls, captures, arity/type checks and cross-fragment function values. Thirteen definition tests also cover ordered publication, failed initializers, namespace directives and explicit declaration-expression/reload restrictions. Primitive dynamic arithmetic now executes for numbers/nil/booleans/UTF-16 strings, with 397 fresh pinned source observations and a 1,024-sample formatting/parsing matrix; first-class arithmetic and seven scalar predicates now use canonical live cells and universal invocation; object conversion and compiled macro/core import remain. Five comparison runtime values and bounded macro expansions now have123 exact primary observations plus five separately recorded old-capture contract divergences; see [comparisons](docs/runtime/comparisons.md). Source loop/fixed-function recur now has lexical tail checks, parallel replacement, capture/type/effect and fuel recovery tests. Named and multiple fixed signatures now execute with self identity, GC/rebinding and exact arity-gap guards. Collections, variadic/destructured signatures, dispatch/effects and production migration remain; see [portable pipeline](docs/runtime/portable-pipeline.md).
- **M2-03 — Runtime ABI v1 and closures** (completed runtime foundation on main; issue #10 auto-closed by acceptance PR #64). Stable shared GC prelude, boxed f64/UTF-16, generic fixed/variadic invocation, live binding cells and rooted values cross independently generated fragments. The 278-case source corpus and 1,024 numeric samples pass; actual manifest/prelude mismatches fail before initializer effects. See [criterion-by-criterion acceptance](docs/roadmap/acceptance-runtime-abi-v1.md). Main and M2 overall remain incomplete; extended source signatures, nominal machinery, complete core and command frontend migration remain separate work packages.
- **M2-04 — Nominal types, protocols and exceptions** (in progress). Descriptor-backed source types, constructors/arrows, fixed protocol overloads, live extension/membership and identity execute across fragments with GC, effects and typed rejection. Ten native nominal tests and 35 added fresh source observations cover selected behavior; [runtime/source evidence and boundaries](docs/runtime/nominal.md) preserve incomplete builtin/native dispatch, general field attributes/runtime metadata, full core/macros. Scoped mutable-field assignment now executes against28 fresh primary observations; immutable/local shadowing and retained captured owners have focused guards. Source throw/try now executes exact values, ordered nominal catches, handler captures and finally cleanup through verified IR; ten native regressions and14 fresh exception observations establish bounded evidence, with ExceptionInfo/public error classes and async cleanup still unfinished. See [exception regions](docs/runtime/exceptions.md). Rooted dynamic bindings, with-redefs and global set! now have source-order, GC, exception and fuel recovery regressions; see [dynamic bindings](docs/runtime/dynamic-bindings.md). Full macro warnings/import and asynchronous context remain unfinished. Descriptor-backed ExceptionInfo constructors/getters now have raw-field/live-class/named-property and GC regressions with 397 fresh source observations; printing/stack, complete Error surfaces and compiled core import remain unfinished; see [ExceptionInfo](docs/runtime/exception-info.md). Native primitive/default protocol tables now have29 fresh independently decoded source observations, captured-method/current-cell/redeclaration and GC regressions; see [native fallback](docs/runtime/native-protocols.md). Literal class/function properties, known instance fields and string/array length now have64 exact source observations and typed guard/GC/order/native coexistence tests; see [named properties](docs/runtime/named-properties.md). Type-method namespace/lexical scope now has14 fresh source observations and compile-atomic recovery guards; see [type method scopes](docs/runtime/type-method-scopes.md). Object blocks and direct dot calls now have52 fresh source observations covering unbound shared method values, receiver/recur, overloads and lookup order; see [Object methods](docs/runtime/object-methods.md). Public prototypes and complete host-property/compiled macro/core acceptance remain open. Issue #11 stays open.

### M3: Persistent development environment

- **M3-01 — Incremental compiled REPL** (planned). Replace source replay with one runtime and compiled input fragments. A native persistent session embedding API and forty-four session tests establish prerequisites; the command frontend, atoms and full acceptance remain; see [session host](docs/runtime/portable-session.md).
- **M3-02 — Namespace loading and redefinition** (planned). Implement live binding cells, namespace loading, defonce and reload semantics. Source graph preparation and eleven executing module tests now establish compiler prerequisites; persistent production clients and reload/cache/privacy policy remain; see [module preparation](docs/runtime/portable-modules.md).
- **M3-03 — Compiled macro bootstrap** (planned). Run macros in a separate compiled phase session and remove the temporary evaluator. Six bounded checked control-macro expansions now establish retained-source prerequisites; this is not compiled macro acceptance. See [control macros](docs/runtime/control-flow.md). Forward declarations now preserve source undefined reads and defonce initialization; see [declarations](docs/runtime/forward-declarations.md). The bounded caching-hash dependency now has38 fresh scalar observations and located recovery guards; full compiled macros and List hashing remain open; see [hash caching](docs/runtime/caching-hash.md).
- **M3-04 — Session lifecycle and interruption** (planned). Define reset, roots, code residency and cancellation while interactive I/O is pending. Native session reset/owned handles/fuel recovery/residency counters now have executing evidence; interactive cancellation and live heap accounting remain.

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
