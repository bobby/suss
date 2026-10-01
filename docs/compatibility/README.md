# ClojureScript compatibility evidence

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
core equality. There are 33 fresh pinned/native value matches and one separately
asserted pinned empty-group parse failure; native rejects that input with a
located compile diagnostic. Actual fragments execute in Runtime and Macro
phases. Selection97/artifacts101 and192 partial/873 unassessed reviews remain
prerequisites. Full compiled macros, complete case/case*, public hash Date
handling, collections and release gates remain unfinished. See
[scalar case evidence](../runtime/scalar-case.md).
