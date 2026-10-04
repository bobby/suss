# ClojureScript compatibility evidence

The native command REPL now uses one persistent portable Session and displays
already-rooted scalar results without source reexecution. Eight process-level
regressions establish bounded frontend persistence/recovery/reset/reader evidence.
[Atom storage](../runtime/atoms.md) adds five partial reviews with real compiled
state and 28 fresh pinned/native observations; neither slice certifies complete
atom compatibility, compiled macros,
namespace reload, cancellation or full printing. See
[compiled REPL evidence](../runtime/compiled-repl.md). The M2 foundation acceptance
is now incorporated on main; future inventory and M3–M9 gates remain explicit.

The [development oracle](../../tests/oracle/README.md) executes the pinned
ClojureScript source in Node and records lossless tagged reference observations.
Its 16-case corpus is shared with independently decoded Suss execution:
**9 differential passing, 7 failing, 0 skipped**. Exact tagged observations and
failure stages are tracked separately from the legacy baseline. Stable known
failures do not establish compatibility; ExceptionInfo implementation and broader
semantic/arity coverage remain M2/M4/M7 work. The bounded M1-02 evidence-harness
acceptance is complete; known failures are not compatibility successes.

The contract is [the design specification](../design/suss-0.3.1.md), using the
pinned ClojureScript submodule. `cljs-core.edn` contains **1,065 source declarations**
from core.cljs and core.cljc, with declaration kind, phase, source range, reader
context and SHA-256. This is a source inventory, not 1,065 implemented functions
or a count of public APIs. Both reader branches are retained; private definitions,
protocols and types are included because portable forms depend on them.

```sh
python3 scripts/cljs_inventory.py
python3 scripts/cljs_inventory.py --check
python3 scripts/cljs_reviews.py
python3 -m unittest discover -s scripts -p 'test_*.py'
```

The scanner does not execute source, descend into macro templates, or infer
portability from host interop text. Review module-level declarations generated
by macros, protocol methods, constructors and conditional branch applicability
before declaring the inventory complete as a public API contract.

`reviews.edn` is the manual overlay, keyed by generated declaration ID. Each
review must record visibility, arities, dependencies, classification
(`:portable`, `:adapted`, `:host-specific`), rationale, implementation status,
adaptation path and tests. Unreviewed entries remain `:unassessed`; they are not
excluded. Source hash changes invalidate the review. Ported source retains EPL
notices and must have reproducible extraction/patch provenance.

## Review overlay schema

`python3 scripts/cljs_reviews.py` validates the pinned source and the strict EDN
data overlay. It rejects stale source hashes, unknown/duplicate IDs, missing or
unknown fields, invalid arities and unsupported schema versions. It parses data
only. The initial empty overlay reports **0 reviewed, 1,065 unassessed**.
A review is not an executing test result; `:tests` lists evidence references for
reviewers to inspect. Full compatibility requires the separate executing suites.

Each `:reviews` entry is keyed by the exact generated declaration ID and contains:

| Field | Required value |
| --- | --- |
| `:source-sha256` | Current declaration hash from the generated inventory |
| `:visibility` | `:public`, `:private` or `:generated` |
| `:arities` | `{:fixed [0 1] :variadic-min nil}` with sorted unique nonnegative arities; nil for a noncallable declaration. defn/defmacro reviews require explicit arities |
| `:dependencies` | Vector of unique dependency names or declaration IDs, retaining phase qualifications where needed |
| `:classification` | `:portable`, `:adapted` or `:host-specific` |
| `:rationale` | Nonempty explanation based on the portable contract |
| `:status` | `:unimplemented`, `:in-progress`, `:implemented` or `:excluded` |
| `:adaptation-path` | Nonempty extraction/patch reference for `:adapted`; nil allowed otherwise |
| `:alternative` | Explicit replacement for excluded host-specific behavior; nil allowed otherwise |
| `:tests` | Vector of unique test/evidence references; at least one required for `:implemented` |

Excluded entries must be host-specific and name an alternative. The validator
does not infer portability, inspect dependency implementation, execute referenced
tests or certify extraction/patch provenance. Protocol methods and macro-generated
constructors remain explicit manual review work; the scanner's 1,065 top-level
declarations are not a public API completeness claim.

The [source and license policy](PROVENANCE.md) applies before importing core forms.

## Prototype baseline

The current legacy corpus has **201 passing cases, zero known failures and zero
skips** after the foundation repairs. This is a small, previously curated corpus;
it does not establish full upstream compatibility.

`cases.json` records every reviewed case ID, expression and expected value. Its
check catches removed passing cases and changed expectations, even when the
failure baseline is empty. This generated data retains the source test suite's
[attribution and EPL notice](../../reference/cljs-tests/README.md).

`known-failures.json` records each currently failing legacy case, its failure
stage and exact diagnostic/decoded output. Ordinary conformance tests fail on
new failures, changed failures, removed cases and unexpected passes. Passing
this baseline test means **no change to reviewed failures**, not full compatibility.

The independent Rust decoder reads the prototype GC heap. It supports scalar
values, keywords/symbols, persistent vectors (including trie nodes), maps, sets,
Cons, IndexedSeq and MapEntry. Unsupported values, including unrealized LazySeq,
are decode errors. It does not invoke the Suss printer or equality to decide
whether a test passes. Decoder layout knowledge is intentionally isolated in
`tests/support/decode.rs` and must change with the runtime ABI.

When intentionally adding/changing test inputs, regenerate `cases.json` with
`cargo test -p suss-compile --test conformance record_case_catalog -- --ignored --exact`
and review removed cases and changed expectations.

To capture observations after intentionally changing semantics:

```sh
cargo test -p suss-compile --test conformance record_known_failures -- --ignored --nocapture
```

Review every diff. Do not accept new failures merely to get a green build. Fix
regressions and remove resolved entries. The shared development differential
oracle now records lossless UTF-16/float transport and effect traces separately from this legacy baseline. Comprehensive
portable semantic/arity coverage and repairs for its exact known failures remain
M2/M4/M7 work.

The conformance loader rejects missing files, empty/malformed suites, duplicate
or unknown fields/IDs, namespaced schema keys and non-Boolean/true skip values.
Baseline/catalog JSON maps reject duplicate keys and trailing data. Executing
negative GC fixtures verify unknown tags, malformed boxes and non-string arrays
fail decoding. Harness regressions also check new/changed failures and unexpected
passes against exact stage/diagnostic records.

## Portable reader foundation

The separate [reader-form boundary](../runtime/reader-forms.md) preserves source
spans, metadata, binary64 and UTF-16. Its 14 scalar observations match fresh
pinned ClojureScript reader execution and are transferred through ABI runtime
intrinsics. They do not change the compiler corpus's 9 passing/7 failing/0 skipped
counts; reader/IR/backend and namespace-phase integration remain incomplete.

## Portable compiler bootstrap evidence

The [new HIR/IR path](../runtime/portable-pipeline.md) compiles the 14 scalar reader
cases into actual ABI fragments and executes a separate 278-case source corpus
whose observations match fresh pinned ClojureScript/Node exactly. It remains a
bounded bootstrap; legacy CLI/AOT/macros and the 16-case full source corpus have
not migrated. The latter remains 9 passing/7 exact failures/0 skips. No inventory
entry is marked implemented by these counts; eleven arithmetic/nominal/dynamic declarations are reviewed as in-progress; 1,054 remain unassessed.

Primitive arithmetic now has executing dynamic number/nil/boolean/string coercion
evidence, supplemented by a fresh pinned 1,024-sample number formatting/parsing
matrix. Object conversion, complete arithmetic macro integration and public
numeric core remain unfinished; these counts do not classify inventory items.
See [numeric build input](../../runtime/numeric/README.md) for pinned dependency,
artifact fingerprints, retained licenses and scratch-memory policy.

The manual overlay now reviews runtime +, -, * and / as public adapted definitions
with explicit fixed/variadic arities, source hashes and macro/reduction dependencies.
All four are in-progress because object conversion and source-backed core/macros
remain unfinished. No item is marked implemented or excluded. The generated
inventory remains byte-exact; source import still requires retained EPL provenance.
See [arithmetic values](../runtime/arithmetic-values.md) for actual executing scope.

Five nominal macro declarations now join the four arithmetic reviews as in-progress adaptations. Their pinned source hashes, dependencies and bounded executing evidence are recorded in reviews.edn and [nominal runtime/source boundaries](../runtime/nominal.md). None is marked implemented; builtin/native dispatch and complete macro/core integration remain open.

Two dynamic macro declarations now join the nine arithmetic/nominal reviews as
in-progress adaptations: 11 reviewed, 1,054 unassessed. Their exact source hashes
and remaining warning/compiled macro/async requirements are recorded in the overlay
and [dynamic binding evidence](../runtime/dynamic-bindings.md). None is certified
implemented.

Five ExceptionInfo constructor/getter definitions now join the eleven existing
reviews as in-progress adaptations:16 reviewed,1,049 unassessed. Raw fields, live
class identity, missing/reordered field names and GC execute; persistent map data,
printing/stack, full Error surfaces and ordinary host-global constructor behavior
remain unfinished. See [ExceptionInfo evidence](../runtime/exception-info.md).

The first [reproducible bootstrap core import](CORE-IMPORT.md) retains exact source,
explicit patches and EPL packaging. The identity review brings the current overlay
to 17 in-progress reviews and 1,048 unassessed declarations. Its one selected form
is separate from complete
core compatibility and the legacy differential baseline.

Seven [scalar predicate adaptations](../runtime/scalar-predicates.md) bring the
current overlay to 24 in-progress reviews and 1,041 unassessed declarations.
The portable source corpus now has 397 inputs, retaining the original 303 and adding 94
primitive type/identity cases. This does not certify compiled predicate macros,
collection equality/hash or complete core compatibility.

Two additional [source-backed bootstrap imports](CORE-IMPORT.md), not and boolean,
bring the current overlay to 26 in-progress reviews and 1,039 unassessed declarations.
Three forms retain source/patch/license hashes and execute against a separate
50-case fresh primary corpus; the portable pipeline corpus remains 397 inputs.
Full core/macro/metadata/collection acceptance remains incomplete.

The next source-backed selection, some?, brings the current overlay to 27
in-progress reviews and 1,038 unassessed declarations. Four retained-source forms
execute against 72 fresh primary cases. Pinned generated code and captured-function
probes show that some? ignores redefinitions of both nil? and not; the explicit
compiler nil-test primitive preserves that behavior. Earlier speculation about
a live not dependency was incorrect. Full macros/metadata/core loading and
collection acceptance remain unfinished.

The original native-satisfies? runtime adaptation brings the current overlay to28
in-progress reviews and1,037 unassessed declarations. A separate29-case primary
corpus establishes native nil/primitive/object/default protocol lookup, live
function/protocol tables, plain native argument filling/truncation, receiver recur
and public helper replacement. Existing portable397 and source-import72 corpora
remain separate. Closure-owned property storage preserves the ten-type ABI and
original callback environments. Full macros/metadata/host properties/collections
remain incomplete; see [native protocol evidence](../runtime/native-protocols.md).

## Mutable array foundation

The [array boundary](../runtime/arrays.md) adapts numeric indexed GC storage,
growth/identity, shallow clones, nested dimensions and bounded macro/runtime
semantics. Its separate56-case primary corpus preserves literal nil/dynamic
undefined fill, expansion order and error effects. Twelve source declarations
add partial hash-bound reviews: current overlay40 reviewed/1025 unassessed.
Named/coerced host properties, checked-array options, source literals and full
sequence/collection/core integration remain unfinished. This does not change the
397 portable or72 source-core corpus counts, close an issue or certify M4.


Two more retained-source forms, inc/dec, have explicit hash-bound defn patches,
original source/docstrings/notices and byte-preserved EPL packaging. The fresh
source import corpus now has117 observations (original72 unchanged);45 added
cases cover first-class scalar coercion, inc UTF-16 concatenation, binary64
rounding, aliases, primitive arithmetic independence and current runtime cells.
The current overlay is42 in-progress reviews/1023 unassessed. Full compiled
macros, object coercion, privacy/metadata and automatic core loading remain open;
no issue or milestone acceptance is claimed by this prerequisite port.


Comparison primitives and bounded macro expansion add ten partial source reviews;
the overlay reached52 in-progress/1013 unassessed in that increment. The separate123-case primary
corpus covers scalar/UTF-16 order and macro/runtime operand effects. Five exact
pinned captured-wrapper TypeErrors remain in an explicit divergence catalog,
with accepted native old-capture behavior tested separately. These are not five
additional matches or skipped successes. Object coercion, full IEquiv/core imports
and compiled upstream macros remain unfinished; see [comparisons](../runtime/comparisons.md).
The existing portable397 and source-import117 corpora remain separate.

Ten additional pinned/native namespace probes preserve the difference between
explicit user comparison refers (automatic core macros remain active) and provider
aliases (user functions execute). These are separate from the123 shared cases
and five retained-capture divergences; the earlier review inference was withdrawn.

The direct protocol `implements?` bootstrap adds one partial hash-bound macro
review:53 in progress /1012 unassessed. Its33 fresh pinned observations remain
separate from earlier corpora. Direct markers differ from native/default fallback;
full compiled macros/core/metadata implementations remain unfinished. See
[implementation predicate](../runtime/implements.md).


Fifteen retained runtime protocol declarations now enter the generated canonical
core artifact without patches. The overlay is68 in progress/997 unassessed;
21 selected forms produce25 licensed, hash-verified artifacts. Separate31 primary
observations and two executing native tests exercise imported interfaces through
an original nominal adapter. These are prerequisites for issue #16/#17, not
collection acceptance. See [core interfaces](../runtime/core-interfaces.md).


UTF-16 indexed storage now supports numeric alength/aget on strings as required
by pinned IndexedSeq. A separate 31-case fresh primary/native corpus covers exact
units, missing numeric indexes, nested order and retained functions after GC and
redefinition. Four existing source reviews are extended; counts remain68/997.
Full string/property/macro and sequence acceptance remain unfinished; see
[indexed strings](../runtime/indexed-strings.md).


Six control macros needed by retained collection source have bounded checked
bootstrap expansions: when/when-not/if-not/and/or/cond. A separate54-case primary
corpus exercises scalar results, operand effects, throws/finally, nominal identity,
tail recur and shadowing. Six new partial reviews bring the overlay to74/991.
No source form is copied, and the25-file source artifact only updates its review
hash. Full compiled macros/core and persistent collections remain unfinished;
see [control macros](../runtime/control-flow.md).


Source forward declarations add one partial macro review (75 in progress/990
unassessed). Seventeen freshly executed primary observations distinguish known
uninitialized variables from unresolved names and preserve defonce/redefinition.
No upstream source is copied; the25-file artifact only updates its review hash.
Complete compiled macros, runtime Var metadata and source core loading remain
unfinished; see [forward declarations](../runtime/forward-declarations.md).


Five additional retained protocols (IStack/IReduce/IReversible/IIterable/IDrop)
bring the current overlay to80 partial reviews/985 unassessed. The canonical
artifact selects26 forms and retains30 generated licensed files. The interface
corpus has65 exact primary/native observations, preserving its original35;
five native tests cover all20 imported protocols, both reduction signatures,
ordered effects, extensions, aliases and captured dispatch after reload/GC.
Actual persistent list/sequence, reduction and iterator implementations remain
unfinished; see [core interfaces](../runtime/core-interfaces.md).


Literal named-property access needed by retained source now has64 independently
decoded fresh primary/native observations plus malformed-storage/recovery guards.
Class/function attributes stay GC-owned and coexist with native protocol keys;
known instance fields and actual string/array lengths preserve their storage.
Two existing partial reviews are extended:80 in progress/985 unassessed. The30
source artifacts only update the review manifest hash. Prototypes, computed/munged
names, extra instance fields and complete source types/core remain unfinished.
Munged instance schemas and known unfinished callable/inherited attributes now
raise explicit errors; full table validation rejects malformed trailing keys;
see [named properties](../runtime/named-properties.md).


Type-method scope now has14 fresh pinned observations and located compile-atomic
recovery evidence. deftype ignores enclosing lexical values while parameters and
physical fields remain available; runtime extend-type and ordinary closures keep
captures. The existing partial deftype review is extended without changing
80/985 statuses or30 imported artifacts; see
[type method scopes](../runtime/type-method-scopes.md). Object methods and complete
compiled macro/core acceptance remain unfinished.


Object methods now have52 fresh pinned/native observations, preserving their
original44, and compile-atomic malformed declaration guards. Fn and fn? add two
partial source reviews (82/983);28 selected forms retain32 licensed artifacts.
The Fn marker branch remains part of the imported predicate. Direct calls preserve
lookup-before-argument order, unbound shared function identity and anchored recur.
Public prototypes, munged properties and complete macros/core remain unfinished;
see [Object methods](../runtime/object-methods.md).


The internal caching-hash macro dependency now has38 fresh pinned/native scalar
observations and compile-atomic invalid/immutable key guards. One partial review
brings the overlay to83/982;28 selections/32 licensed artifacts change only their
review hash. Complete List/Cons hashing and compiled macros remain unfinished;
see [hash caching](../runtime/caching-hash.md).


Bitwise hashing prerequisites now have70 exact primary/native observations plus
namespace, arity, ordered coercion and recovery guards. Thirty additional partial
reviews bring the overlay to113/952. Retained bit-count and int-rotate-left add
two licensed forms:30 selected forms/34 artifacts. Captured JavaScript off-arity
behavior remains an explicit divergence under the accepted error contract; no full
public/core/collection acceptance is claimed. The conditional imul provenance
record is checked against the pin and exact complete form bounds in CI by
`scripts/bitwise_provenance.py`. Four pinned public-wrapper errors and five separate
internal-body diagnostics preserve the captured variadic/live reducer boundary;
these are outside the70 equal public observations. See [bitwise hashing](../runtime/bitwise-hashing.md).


Retained scalar Murmur algorithms/constants and zero? now have57 fresh primary/native
observations and located namespace/arity/capture/GC recovery guards. Thirteen new
partial reviews bring the overlay to126/939. Ten retained forms bring selection
to40 forms/44 licensed artifacts, with explicit fixed defn patches preserving
algorithms and metadata. Bounded threading/strict-zero expansions are prerequisites,
not compiled macro acceptance; full collection/string/numeric hashing stays open.
See [scalar Murmur hashing](../runtime/murmur-hashing.md).


## Cached string hashing

Retained runtime js-obj/cache declarations/add-to-string-hash-cache/hash-string
bring the import to50 selections/54 licensed artifacts and the overlay to139
partial/926 unassessed. All64 preserved source observations now match fresh pin
and independently decoded native execution; source3/private adapters2/ABI40 and
Python76 pass. [Evidence and boundaries](../runtime/cached-string-hashing.md)
distinguish the first-class runtime factory from the unfinished literal js-obj
macro and complete bracket/foreign-object/public hash acceptance. Full workspace,
independent PR review and final-head CI remain required. No milestone closure.


## Retained sequences and persistent rest

The source-backed [sequence/list foundation](../runtime/sequences.md) selects68
forms/72 licensed artifacts and records157 partial reviews/908 unassessed. All
methods of List/EmptyList/Cons/IndexedSeq are retained; explicitly pending helpers
remain uninitialized. Fresh75 primary/native observations preserve all69 pre-review cases (including the original51),
including canonical empty literals, real persistent rest, shared tails, live array
views and UTF-16 units. No full method/equality/hash/reduction/iterator/core or
milestone acceptance claim. Independent review/full/final-head CI remain required.

## Retained sequential equality

Pinned `=`/equiv-sequential and complete native number/default IEquiv setup now
execute against131 fresh primary/native sequence observations (prior75 unchanged).
The import selects70 forms/74 licensed artifacts and records159 partial reviews/
906 unassessed. Six native tests cover arity/effect/typed throw/GC recovery;
[evidence](../runtime/sequential-equality.md) records the initial new-probe failures
and boundaries. Other persistent types, collection hashing/reduction and compiled
macros remain unfinished. No issue/milestone completion claim.

## Retained reduction

Complete source IDeref/Reduced and ten reduction/helper declarations now select82
forms/86 licensed artifacts with172 partial/893 unassessed reviews. Fresh65
reduction and81 control observations match actual primary/native execution;
[evidence](../runtime/sequence-reduction.md) retains initial setup failures and
explicit destructuring/compiled macro/transducer/collection boundaries. No issue
or milestone completion claim; full/review/final-head CI gate readiness.

## Retained iteration and reverse views

Complete IndexedSeqIterator/RSeq and reversible?/rseq retain all methods and
source provenance:86 selections/90 licensed artifacts,176 partial/889 unassessed.
Fresh59 primary/native observations and2 native tests cover storage, UTF-16,
metadata, equality/reduction and GC. A separate exact primary1 length-write case
requires a native typed adapter error, not matching value or skip;
[evidence](../runtime/sequence-iteration.md) preserves that boundary and initial
failures. Hash/printing/index helpers, generic reverse, other collections and
compiled macros remain unfinished; no issue/milestone completion claim.

## Identity hashing and metadata dependencies

The private owner-held identity adapter supplies stable runtime-local UIDs for
closures, descriptors, objects and errors. ABI2 appends UID storage and rejects
old artifacts before initialization. Complete pinned IFn/MetaFn/with-meta/meta
forms bring selection to96 forms/100 licensed artifacts and the overlay to189
partial reviews/876 unassessed. Fresh34 primary observations,33 matching native relations and1 exact strict-arity
contract boundary across4 native tests cover identity, metadata, GC, errors and
phase guards. Independent review corrected IFn receiver/arity dispatch; see
[identity hashing evidence](../runtime/identity-hashing.md). Public/default and
collection hashing, general IFn call syntax, apply and full metadata acceptance
remain unfinished. Full baseline, independent review and final-head CI still
gate PR readiness; no issue or milestone is complete from this work alone.


Scalar `case` bootstrap now supports grouped binary64/UTF-16 literals and bounded
boolean/nil equality tables with selector-once evaluation and live qualified
core equality. There are 40 fresh pinned/native value matches and one separately
asserted pinned empty-group parse failure; native rejects that input with a
located compile diagnostic. Actual fragments execute in Runtime and Macro
phases. Selection97/artifacts101 and192 partial/873 unassessed reviews remain
prerequisites. Full compiled macros, complete case/case*, public hash Date
handling, collections and release gates remain unfinished. See
[scalar case evidence](../runtime/scalar-case.md).


Retained public scalar hash now preserves all pinned source branches, including
an explicit descriptor-backed numeric Date storage adaptation. Fresh42 pinned/
native observations and4 native tests cover scalar bits, UTF16 composition,
protocol priority, default identity and Date normalization/GC. Selection98/
artifacts102 and193 partial/872 unassessed reviews remain prerequisites. Full
Date/Inst/reader/printing, collection composition, compiled macros and release
gates remain open; see [public scalar hash evidence](../runtime/public-scalar-hash.md).


Retained ordered/unordered collection hash helpers and the empty unordered hash
initializer now execute through the retained source pipeline. Fresh55 pinned/
native observations and5 native tests cover sequential hash agreement, nested
values, UTF16, duplicates/count, caches/metadata, effects and GC recovery. Core
selection101/artifacts105 and196 partial/869 unassessed reviews remain
prerequisites. Persistent vectors/maps/sets/map entries, collision nodes and
compiled macro/release gates remain required; see
[collection hashing evidence](../runtime/collection-hashing.md).


Vector trie prerequisites now retain complete source5575–5619/5648–5670:
VectorNode, pv-fresh-node, pv-aget, pv-aset, pv-clone-node, tail-off, new-path,
push-tail, do-assoc and pop-tail. Selection111/artifacts115;206 partial reviews,
859 unassessed. Private defn bootstrap visibility remains unfinished, and these
helpers do not establish PersistentVector, map/set or M4/M7 acceptance.
[Execution evidence](../runtime/vector-trie.md).


## Collection literal compiler foundation

Vector/map/set expressions now lower through explicit HIR temporaries and existing
constructor/member/array interfaces. Fresh19 pinned observations and independently
decoded native results record15 shared values plus4 exact observations of the
accepted textual-order variance, with0 skips. Six native tests cover threshold
interfaces, lookup capture, entry exceptions, GC and compile-atomic diagnostics.
These use development-only constructor fixtures; persistent collection types,
keyword/symbol literals, quoted collections, runtime metadata and complete M2/M4
acceptance remain unfinished. No inventory status changed. See
[collection literal evidence](../runtime/collection-literals.md).


## Quoted identifiers for compiled macro data

Real source-backed Symbol/Keyword and nested list literals now execute through the
portable pipeline. Fresh48 pinned/native observations include exact type fields,
UTF16, signed hash caches, equality/names and runtime-computed hash agreement.
Selection128/artifacts132 and223 partial reviews/842 unassessed remain prerequisites;
compiled macros, complete collections/metadata/printing are not certified.
[Quoted data evidence and provenance](../runtime/quoted-identifiers.md).

Source defmacro execution now has a partial pinned review (224 reviewed/841
unassessed). Explicitly registered macro functions execute in the isolated Store
and expand inside real lexical analysis; this is not complete macro bootstrap.
[Source macro evidence and limitations](../runtime/compiled-source-macros.md).


Transient hash-map lookup now enforces the accepted post-persistence lifecycle
in both arities, documenting the pinned source variance. Twelve fresh active
lookup observations preserve omitted undefined versus nil behavior in both phase
Stores. This remains a partial review with full M3/core acceptance unfinished.
[Evidence and provenance](../runtime/compiled-macro-hamt-lookup.md).

## Macro environment compiler facts

Actual lexical declarations/initializers/shadows, phase namespace declarations and
immutable source origins now have focused executing evidence. Eight pinned source
position observations match native artifacts in both Stores, including metadata
prefixes and UTF-16 columns. These host inspection fixtures are prerequisites;
compiled source macros still do not receive rich `&env`. No inventory item or M3
acceptance criterion is marked complete by this work. See
[compiler facts](../runtime/compiled-macro-environment-facts.md).

Actual field/shadow records and three-way analysis contexts have focused executing
evidence. Twenty-four ordered pinned context observations and fifteen executed result
strings agree in both Stores, including bare try context and finally/catch/body
macro expansion order. This is compiler-fact evidence; full source-level `&env`
and M3 acceptance remain open. See
[context and field facts](../runtime/compiled-macro-analysis-context.md).


Actual named function scope records now retain declarations, origins, phase
namespaces, shared parent scopes and lexical/field shadows. Definition hints
create no lexical ID; explicit self names retain their actual compiler binding.
Focused native execution and independent review cover macro expansion, scope
restoration and unknown origins. Rich source-level `&env` remains unfinished; see
[function scope facts](../runtime/compiled-macro-function-scopes.md).

Physical method parameter slots now retain separate source role facts. Six exact
pinned observations and four executed results cover receiver/argument/field
shadows and nested scope restoration in both Stores. This is compiler-fact
preparation, not source-level rich `&env` completion; see
[method binding roles](../runtime/compiled-macro-method-roles.md).

Actual analyzed syntax is now retained alongside HIR for rich environment
preparation: [source analysis records](../runtime/compiled-macro-source-analysis.md).
The records retain reader/expansion forms and actual context/phase/origin;
compiler-only lowering nodes have no source record. Executing focused evidence
passes 9 native macro and 38 compiler tests. This does not certify a complete
source AST, inference, rich source-level `&env`, bootstrap or M3 acceptance.

Source records now preserve immutable lexical/namespace snapshots and resolved
declarations. Native bounded form construction preserves canonical data, sharing,
metadata normalization, exact scalar storage and Store identity without compiling
transport fragments. Sixteen focused native tests pass; a separate fresh upstream
rich environment oracle checks sixteen observations and thirteen executed results.
The required full workspace baseline passes 981 tests with zero failures and
17 existing ignores across 101 groups, including all 47 runtime ABI tests.
Source macro `&env`, memoized environment graph construction, complete AST/inference,
original M3 acceptance remain unestablished. Subsequent focused set transport
evidence is recorded below; it does not establish the complete environment schema. No
inventory declaration is reclassified on this preparatory evidence.

Queued native graph transport has three executing regressions for shared binding
identity, a 96-binding chain, declaration environments, bounds and once-only
effects. The native representation keeps explicit backend facts; it does not
establish the portable source `&env` schema, AST inference or invocation contract.
See [analysis graph](../runtime/compiled-macro-analysis-graph.md). Original M3
acceptance remains open and no inventory declaration is reclassified.

Staged declaration preparation now retains actual definition initializer syntax
before body analysis and actual named function syntax before method expansion.
An executing native graph query inspects both fixed and variadic bodies and then
executes both signatures in each Store; it fails with the prior graph transport.
This does not certify portable function metadata, inference or source `&env`.


Retained set prerequisites now include pinned ISet/ITransientSet,
PersistentHashSet/TransientHashSet, HashSetIter and KeySeq/keys selections with
explicit source adaptations and retained EPL notices. Import verification covers
261 files; each new review remains in-progress. Native construction and decoding
retain canonical Store descriptor identity, metadata and bounded HAMT traversal.
Namespace `:excludes` now carries a real persistent set. Literal sets above eight
entries use the retained HAMT factory, while source effects preserve textual
order (the accepted variance from pinned reader hash iteration).

Expanded focused execution passes 18 tests: seven graph, four existing HAMT
sequence, six set and one Error predicate regression. A separate ABI test rejects
copied descriptor identity after GC. Generic ES6/printing helpers, exact duplicate
error messages and arbitrary custom KeySeq cursors remain pending. A fresh force-compiled pinned ClojureScript corpus matches 39 observations
exactly, and independently decoded native results agree in both Stores after GC.
The first native attempt failed on unimported public empty/vec helpers; the final
focused corpus invokes the retained -empty method and transports keys directly.
Those public helpers remain pending. The required workspace baseline for this set prerequisite source passes
998/0/17 across 105 groups. Original M3 environment/bootstrap acceptance remains open. No declaration is reclassified as fully implemented.


Native compiler namespace data now preserves explicit source use/rename roles,
including unchanged-name and dual-role referrals, required-library identity
entries and separate macro import maps. Fresh pinned evidence preserves nullable
shape and field presence across 18 map fields in three source namespaces; native
compiled queries match the shared expected corpus in both phases after GC. See
[namespace graph data](../runtime/compiled-macro-namespace-data.md). This does not
certify default/reload/implicit namespace policy, full declaration/AST/inference
schema or actual source macro &env invocation. No inventory declaration is
reclassified and original M3 acceptance remains open.


Macro graph namespace timing now preserves the enclosing top-level snapshot
separately from the live resolution catalog. Fresh primary observations retain
nested/redefinition timing, staged function metadata and exact named self binding
records; executing native queries verify snapshot visibility and identity through
initializer/function environments after GC. See
[declaration observations](../runtime/compiled-macro-declaration-observations.md).
Complete declaration schema, AST/inference and actual implicit &env remain open;
no inventory declaration is reclassified as complete.

Source callable preparation retains actual validated methods before runtime
wrapping and duplicate-arity elimination, exposed as explicit backend graph
facts. A focused executing regression checks metadata, signature order, aliases,
shared identity and invocation in both Stores after GC. A separate development
oracle records 32 pinned analyzer observations and 11 executed projections,
showing why portable inferred tags must be independent of runtime storage types.
See [source records](../runtime/compiled-macro-source-analysis.md). Native portable
inference and complete function/declaration schemas remain pending; no inventory
item or milestone gate is reclassified.

Compiled source macros now receive actual rooted compiler graphs as implicit
`&env` after `&form`. Executing lexical/function/snapshot regressions pass in both
caller phases; a second fresh pinned source-macro fixture matches three shared
projections and four runtime results. Actual definition syntax and analysis
completion remain distinct from runtime initialization. See
[source invocation](../runtime/compiled-macro-source-environment.md). Complete
portable AST/declaration/inference and bootstrap/lifecycle acceptance remain open;
no inventory definition is reclassified as complete.


Compiled source macro tag projections now match 32 pinned source observations
and 11 executed results in both caller Stores after GC. HIR retains source
inference independently from storage, including unknown return-field presence.
Quoted union sets now lower as data through existing retained constructors,
with nested syntax, metadata and size-boundary execution. Complete source AST,
declaration/function/method schema and remaining inference rules stay open; no
inventory declaration is reclassified as complete. See
[source inference](../runtime/compiled-macro-source-inference.md).

## Compiled declaration metadata

All 18 selected fields of the unchanged 29-case declaration corpus now have an
executing snapshot/catalog comparison in both caller phases after GC. The
projection distinguishes namespace records from resolved-var AST records,
preserves provisional symbol metadata and completed reader provenance, and
retains known direct-function return tags. Source paths are independently
asserted before oracle normalization. No declaration is reclassified as fully
implemented; complete AST/schema, caches, evaluator retirement and original M3
acceptance remain open. See [declaration metadata](../runtime/compiled-macro-declaration-metadata.md).

## Named function local method records

Both unchanged pinned named-self observations now match all eight selected local
fields in both caller phases after GC. Actual staged parameter bindings supply
self callable flags, arity and method parameters; argument records retain unknown
tags as present nil. Fixed/variadic calls execute correctly. The selected binding
view does not certify whole ASTs or complete portable environments, and original
M3 acceptance remains open. See [self-local method evidence](../runtime/compiled-macro-self-local-methods.md).

## Compiled macro reader input metadata

Eight exact pinned reader-metadata projections match executed native source
macros in both Stores after GC, with a separate loaded-file provenance assertion.
Ten focused tests cover location/prefix/conditional/tag behavior, absence on
expanded syntax, snapshot errors, compile isolation and metadata expressions
remaining data. Affected suites pass27/0/0; the reader passes30/0/0 and Python
checks108/0. See [reader metadata evidence](../runtime/compiled-macro-reader-metadata.md).
These are partial macro prerequisites; no inventory item or M3 acceptance gate
is reclassified as complete. Full portable schema/inference, bootstrap and
lifecycle remain open; independent review/full final-head checks are pending.

Retained LazySeq/IPending are an executing dependency for compiled syntax quote.
The complete pinned type/protocol declarations retain source hashes and EPL
packaging. Twelve actual pinned/native shared observations cover deferred and
repeated realization, metadata and failed-thunk retry in both caller Stores.
Subsequent retained concat/chunked helpers and constructors now have50 fresh
pinned/native observations. Compiled syntax quote has18 fresh shared execution
observations, actual reader provenance and staged gensym state. Review inventory
remains371 in-progress/694 unassessed; no declaration becomes completed from
these tests. A live-cell coercion edge, full baseline/review/CI and original
M3/M4 gates remain open; see [reader integration](../runtime/syntax-quote-reader.md)
and [lazy sequence evidence](../runtime/lazy-sequences.md).


Repeated declarations preserve selected source declaration records without replacing completed callable facts. Actual pinned/native evidence and remaining limits are recorded in [declaration preservation](../runtime/compiled-macro-repeated-declarations.md). This does not complete portable environments or M3 acceptance.

Native expression/file execution and bounded canonical value display now have
focused before/after evidence for compiled macro phases, GC data and runtime
compile-error isolation. No declaration is reclassified as fully implemented;
full printing, remaining frontend/evaluator migration and original M3 acceptance
remain open. See [native entry points](../runtime/compiled-native-entrypoints.md).


Selected source tag/return observations now distinguish false/nil metadata, dynamic scalar/function records and provisional/completed invocation information. Actual pinned/native before/after evidence and remaining limits are in [source hint boundaries](../runtime/compiled-macro-source-hint-boundaries.md). Full portable environments/inference remain open.

The [source artifact cache](../runtime/source-artifact-cache.md) has four focused
compiler checks and four executing native tests. Macro expansion effects run
before every lookup; loaded source versions remain stable until explicit reload.
The binary64 test executes two distinct NaN payloads and a real cache hit.
This cache evidence does not certify published user artifact manifests, evaluator
retirement or complete M3; independent review/full baseline/final CI are pending.

[Artifact identity evidence](../runtime/artifact-identities.md) includes the
actual before-fix acceptance of an incompatible compiler build, then rejection
before bindings change, independently decoded old17, actual macro version
records and compiled execution. Compiler3, native2+cache6, lifecycle6 and
affected54 pass; Java-free bootstrap4 and Python118 pass. Review/full/final CI
remain pending. This does not certify full published dependency loading or M3.

[Core binding artifact preparation](../runtime/compiled-core-bindings.md) moves
canonical bootstrap cell initialization into Wasm and includes unbound source
cells for AOT assembly. Its three direct compiler tests,72 affected native
tests,7 lifecycle guards and Java-free bootstrap4 pass. Independent review/full
baseline/final CI remain pending; no declaration or M3 acceptance gate is
reclassified. Existing retained-source provenance and
upstream license obligations remain unchanged.

[Portable AOT component assembly](../runtime/compiled-aot-components.md) now has
five executing compiler tests for scalar component exports, source ordering/live
cells, GC, phase guards and independently decoded boundary exceptions. A shared
source-var export regression fails on the old duplicate import and passes after
import sharing. This is a development scalar boundary; full selected-WIT adapters,
CLI compile migration, evaluator retirement and original M3 acceptance remain open.

[Shared script/AOT source preparation](../runtime/compiled-aot-source-preparation.md)
has two executing source-to-component regressions for inline compiled macros,
Runtime dependencies, provenance, effects/GC and deferred language throws.
Twenty-four existing native tests pass, including script macro rollback and
completed dependency preservation. This is focused preparation evidence; complete
frontend/WIT and original M3 acceptance remain open. No declaration is reclassified.

The original Rust AOT adapter now has focused executing small integer boundary
evidence: six signed/unsigned types round-trip extrema through ordinary binary64
values; fractional/nonfinite/out-of-range/nonnumeric results decode as boundary
language errors. All8 compiler AOT tests pass, including old scalar expectations
and exactly66 source calls after GC. Java-free reproduction/bootstrap4, Python118 and affected CLI60/compiler15 pass;
full baseline/review/final CI remain pending. No upstream forms or shared GC layout changed; complete WIT/frontends
and M3 acceptance remain open. See [AOT components](../runtime/compiled-aot-components.md).

The scalar AOT adapter now preserves exported interface instances, including
versioned package names and inline/named aliases. All11 compiler AOT and4 CLI
source-preparation tests pass; the new source fixture executes a real compiled
macro through a versioned interface. Empty instances, shared cells, initializer
effects, fresh Stores and GC have executing evidence. Java-free reproduction,
bootstrap4 and Python118 pass. Independent review/full baseline/final-head CI
remain required. Interface types/external-id, complete WIT/frontends and M3
acceptance remain open; no core declaration is reclassified. See
[AOT components](../runtime/compiled-aot-components.md).

Independent exported-interface review repaired named-alias `implements` metadata
and rejects function external IDs instead of silently dropping them. All thirteen
compiler AOT tests pass, including parsed-binary canonical-version assertions and
actual alias execution. Named aliases require Wasmtime's component-model implements
feature; the CLI component runner enables it. Full baseline and repaired-head CI
remain pending. Interface types, external IDs, imports and complete M3 acceptance
remain open; see [AOT components](../runtime/compiled-aot-components.md).

Native CLI file compilation now uses isolated compiled source preparation and
portable component assembly, with WIT package/world selection and explicit
export mappings. Three actual command/artifact regressions pass, including typed
interface invocation and rejection of malformed inputs previously coerced to zero.
Affected CLI15, Python118 and Java-free reproduction/bootstrap4 pass. Independent
review, unfiltered full baseline and final-head CI remain pending. Namespace,
project, main and component-host migration, evaluator retirement and original M3
acceptance remain open. See [native file compilation](../runtime/compiled-aot-source-preparation.md).

Native namespace compilation now shares portable file-mode source preparation
and component assembly. Three executing regressions pass for canonical source
lookup, portable conditionals, imported compiled macros, separate phase effects,
GC/fresh Stores, pre-effect declaration errors and deferred Runtime exceptions.
Both regenerated bootstrap images reproduce without Java; bootstrap4 passes.
Independent review/full baseline/exact-head CI remain pending. Explicit export
mappings are required; project/main/component-host migration, evaluator retirement
and original M3 acceptance remain open. See [namespace entrypoints](../runtime/compiled-aot-source-preparation.md).


[Compiled export metadata](../runtime/compiled-export-metadata.md) has four executing
native regressions and seven fresh pinned analyzer observations. File/namespace
shorthand selects only unambiguous freestanding functions; interface mappings stay
explicit. Raw macro namespace metadata remains separate from target export names.
Both images reproduce without Java; bootstrap4 passes. Affected28 and Python118 pass; review/full
baseline/final-head CI remain pending. No declaration or M3 gate is reclassified.


[Native project compilation](../runtime/compiled-aot-projects.md) now shares staged
compiled source preparation and selected-WIT assembly. Five actual command/artifact
tests pass for configured entries/mappings, explicit core source, retained target groups/shared roots,
output preservation and deferred exceptions. Affected CLI37, configuration4,
Java-free reproduction/bootstrap4 and Python118 pass. Compiler module/phase14 and final image reproduction/bootstrap4 pass;
independent review/full baseline/final-head CI remain pending. No core declaration
or original M3 acceptance gate is reclassified.


[Asynchronous scalar AOT exports](../runtime/compiled-aot-async-exports.md) now
execute canonical callback completion over ordinary compiled source bodies. New
compiler2 and native CLI2 tests pass, alongside existing compiler AOT13 and affected
CLI24. Both phase bootstrap pairs reproduce without Java; bootstrap4 and Python118
pass. Public async type flags, zero hidden imports, GC/fresh Stores, captured/live
cells, scalar bits and independently decoded language errors are checked. Review,
unfiltered full baseline and final-head CI remain pending. Suspension, official
command arguments/bindings, evaluator retirement and M3 acceptance remain open.
No inventory declaration is reclassified.

[Official command compilation](../runtime/compiled-official-command.md) now has
focused executing draft evidence for the exact `wasi:cli/run@0.3.1` async result,
ordered string arguments, normal completion and uncaught source failure. Compiler3
and native command5 pass, including Unicode retained in source cells across buffer
reuse, GC and fresh Stores, compiled macros/source dependencies and output guards.
Affected CLI31, allocator/core4, Java-free image reproduction/bootstrap4 and
Python118 pass. Explicit exit status0/73/255 and initializer19 execute; invalid status
values raise catchable language errors, and the compiled Macro phase rejects this
Runtime capability. Complete capabilities, evaluator retirement,
source suspension and original M3 lifecycle gates remain open. Independent review,
unfiltered full baseline and final-head CI are pending for this draft. No declaration
or milestone acceptance gate is reclassified.
