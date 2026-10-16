# Resolved global source AST facts

Initializer records for genuine resolved global symbols expose `:op :var`,
qualified `:name`, canonical `:ns`, and `:info` copied from the captured
`SourceBinding::Global` declaration revision. Resolved operation/name/namespace
override raw declaration metadata in this copy. Each resolution has a fresh outer
`:info` map and shares the captured declaration revision's final nested `:meta`
value, including function `:top-fn` metadata overlays. This preserves the pinned
shallow merge identity without changing reader occurrence construction elsewhere.
Across calls, the same captured declaration revision must retain its pinned nested
metadata identity; equal data from distinct revisions remain distinct. Outer
resolved `:info` maps remain fresh. Source-owner identity alone cannot justify
reusing guest storage: reachable backing arrays can be mutated. Any sharing
mechanism must also preserve the original source facts, live construction
callbacks, cycle/layout checks and cumulative materialization budget. Rebuilding
fresh metadata alone is insufficient because it loses cross-call identity.
The namespace catalog retains
its original record, including raw metadata overrides. Global AST tag presence
and value come from resolved info, including false; lexical inference is unchanged.
Runtime values, physical HIR and source reexecution do not determine these facts.

Eleven fresh pinned observations and all eleven executed Node projections cover
scalar/qualified/hinted/dynamic/function/declared references, raw metadata overrides,
core aliases and old/new declaration revisions. The native parent regression
reproduced absent var identity/info fields. The initial new native helper also
exposed unavailable public `symbol`/`mapv` functions; the bounded projection now
receives the unqualified declaration key as a raw macro argument and explicitly
selects the same fields in both implementations. Fresh force/cache-disabled
analysis and actual Node execution revalidate every unchanged expected observation.
This establishes the projection, not support for those unimplemented core APIs.

The repaired graph initially failed the core case because the identity adaptation
patch omitted its upstream docstring. The patch now retains the original body and
docstring in the actual compiled declaration. Source extraction/hash, patch hash,
review overlay and generated manifest retain provenance and the EPL notice/license.
Both Runtime and Macro bootstrap image pairs were regenerated; Java-free
reproduction and all four executing bootstrap tests pass before native acceptance.

All 58 affected native tests pass across nine groups (graph11/bindings8/scalars3/
globals1/locals2/env3/tags2/syntaxquote11/core-import17). The eleven global cases run
in Runtime and Macro caller Stores after GC, with only the exact two core resolved
name/ns spellings adapted to the design's canonical `suss.core`. Old declaration
metadata remains captured after redefinition, while the live value changes; the
initializer effect occurs once. The extended focused test also passes: a global
callee invocation does not inherit its callee's var identity or a fabricated
constant value, and its effect occurs once. All123Python checks pass, including
five observation-corruption checks that distinguish numeric1 from true metadata.

The source contract follows pinned ClojureScript
`c4295f303100bbf5afac449242d30bca1126f1a1`, `analyzer.cljc`'s
`analyze-symbol`4149–4165, `resolve-var` and default `resolve*`1248.
Analyzer SHA-256:
`297802c627474434f1ef868e31f5f9913c290a4e80c509a40c704dced95bbf47`.
Identity source SHA-256:
`4997a405f43df040b92909ac831f031385fd1ac86948cb1be06dd3529323504e`.
[Identity adaptation](../compatibility/patches/identity.json),
[generated manifest](../../runtime/core-import/manifest.json), and
[EPL license](../../runtime/core-import/epl-v10.html) remain distributed.
Native graph transport and observation helpers are original code; JVM/Node remain
development oracles with no shipped dependency.

Fresh forced/cache-disabled primary analysis and actual Node execution also
establish four cross-call metadata identities `[true true false false]`, with
before/before/after/after documents and live value17. The earlier native execution preserved
all four observations in both caller phases after GC, keeps the old metadata
accessible after redefinition, confirms outer info is fresh across calls, and
rechecks unchanged-revision sharing after reset. A Macro Store graph regression
forces GC, rejects warmed node/UTF-16/work budgets, distinguishes equal metadata
from different revision identities, prunes dead source owners and recovers.

Field/quote/compound/invocation/children ASTs, constant-expression policy, complete
source declaration/function/method/inference schema and evaluator retirement remain
unfinished. Dependency/target integration and rooted pending-I/O cancellation plus
code/live-heap accounting remain original M3 gates. Independent review, significant
fixes, the unfiltered full workspace baseline and exact final-head CI are required
for this increment. No issue closure, merge or milestone completion.

The cumulative-fuel prototype retains declaration metadata by captured global
and DefinitionInfo revision, with independent logical, construction/storage,
class, prototype and protected-memo validation. Invalid old candidates rebuild
from native source facts; arbitrary old guest objects are never repaired. The
outer info maps remain fresh. Failed, unsupported or poisoned fresh candidates
are not retained. This controller is a candidate implementation: broader fuel
regressions still fail and full acceptance remains unproven.

During reconstruction, valid retained candidates are represented by fresh private
placeholders. Static placement checks prevent their passage to hashing/equality;
actual input taint selects private factory parents, while ordinary untainted
construction preserves callbacks. After callbacks, candidates are rechecked and
native exact-slot fixups replace placeholders before publication. Only immediate
untainted inputs of actual tainted parents become opaque fixup frontier roots;
per-occurrence tracking preserves repeated reader construction and metadata
cloning. These cuts avoid scanning unrelated subtrees and supply no source
certification. Source validation and cancellation/fuel charging remain required.

The component notes below describe individual primitives and historical evidence;
a primitive alone does not establish the integrated reuse contract.

After authorized shipped-bootstrap provisioning, a private native registry now
captures the actual canonical IHash descriptors, key and method closures. It
checks live installation identity and exact key, schema and closure storage with
metered, cancellation-aware native observations. It does not refresh from mutable
globals; unrelated protocol dispatch and live hash-helper bindings remain live.
Internal roots have separate accounting from externally held Session values.
Failed staging publishes no registry, and reset creates new identities.

These are physical installation observations, not source-data certification.
Legitimate UID/property changes can currently make an installation ineligible.
The registry alone cannot mint a memo receipt or certify retained source data;
the integrated controller requires the independent checks described above.

Installation inspection also accepts an active Wasmtime caller context, debiting
its existing fuel without starting another operation or replenishing fuel.
Callback registration must bind the Store, runtime instance, roots, identity and
interrupt flag from the same Session; the supplied numeric identity alone does
not prove that relationship. A Store-bound native callback and executed Wasm
driver exercise repeated calls, exhaustion, interruption and recovery. The new
memo forwarding bridge also executes a Wasm-to-native-to-runtime exception
boundary, comparing actual error descriptors and UTF-16 payload messages after
GC against an ordinary-setter control. Guest catch handlers and nontrivial
dynamic bindings remain unexecuted for that bridge.

Bootstrap generation builds a private plan borrowing the exact verified IR
functions and memo-store instructions. Macro emission consumes its recorder,
rejecting cloned or wrong-parent instructions, duplicate selection, missing
coverage and errors followed by otherwise complete selection. The25 protected
store calls preserve receiver/result locals and carry their source-order site
IDs to a native bridge, which charges the active allowance and forwards to the
ordinary runtime setter. Runtime emission retains ordinary setters for portable
AOT providers. Ordinary compilation has no private mode, and Session admission
reserves the integrity import namespace. The native bridge now also stages
protected-store receipts; source reuse additionally requires factory-grounded
write-time domains and complete final validation after callbacks.

The development-only [CLJS-hosted analyzer probe](../../tests/oracle/src/suss_oracle/declaration_metadata_retention.cljs)
separates fresh reader construction from analysis of pre-read declarations,
repeated resolved-var analysis and explicit set/map construction controls. Its
[recorded observations](../../tests/oracle/declaration-metadata-retention-observations.json)
come from the clean pinned compiler executing in Node, with structural guards
on the actual initializer/info/metadata path. The old declaration metadata and
nested audit set retain identity across resolutions; outer info maps differ and
a new declaration revision supplies distinct metadata. The selected numeric
hash trace is empty during repeated resolution; fresh nine-element reader sets
and explicit set/map controls each record101 through109 once in order. Total
public hash counts are also retained, including unrelated analyzer work.

Run `scripts/probe-declaration-metadata-retention.sh` to observe the pin afresh.
This probe executes the analyzer as ClojureScript, avoiding substitution of JVM
hashing behavior. It does not execute Suss's additional native-to-guest transport
and therefore does not establish a transport callback-count policy or authorize
removing independently established reader-occurrence effects. Its total internal
hash counts are raw evidence, not Suss acceptance targets. Retained candidates
still require independent source expectations, authentic memo validation and
revalidation after any callbacks that can mutate their reachable storage.

A separate native logical-source witness now normalizes immutable reader metadata
before observing guest contents. Its callback-free finite decoder and native
comparison preserve exact number bits, metadata and map key/value associations,
while comparing map/set iteration order independently. Spans are excluded. The
raw reader-occurrence interner retains its existing ordered semantics; unordered
comparison uses a separate mode and charged fallible sorting. Ambiguous source
key cases decline admission rather than inventing portable key equality: the
initial subset requires unique string/keyword map keys and nil/boolean/string/keyword/finite-number set
elements. Compound keys, nonfinite numeric set keys and duplicate-equivalence
cases still need normalization before this can support the complete contract.

Actual Session regressions exercise source17 versus a construction callback's99,
including a freshly captured physical snapshot that accepts the poisoned object.
The independent logical witness rejects it without repairing the old object.
Other checks cover reordered pairs, forged duplicate storage, metadata layers,
exact zero/NaN bits, lazy-thunk suppression, cycles, foreign values and shared
fuel/interrupt recovery. Witness construction preserves the original typed
metering error across reader APIs that otherwise return compile diagnostics.
Unexpected struct widths are rejected before native field collection.

This primitive is exercised separately. Its use by the controller does not
replace the independent checks: it deliberately does not validate operational representation, aliases,
hash/UID fields, properties or unused backing storage. Identifier FQN
and the compiler's enumerated set backing state now have the specific checks
below; those do not establish complete storage authentication. A forged
memo still matches its logical source; the regression makes that limitation
explicit. No logical match authorizes receipts or reuse. The existing warmed
metadata identity requirements remain unchanged. Final metadata-recipe facts,
complete representation/storage validation and authenticated memo transitions
must still be combined, including validation after construction callbacks.

Final metadata expectations are now captured after declaration field overlays
and Alias creation, with the exact native Global and DefinitionInfo revision.
Preparation is a graph method, so it takes that graph's own recorded root and
recipes. Its iterative projector constructs owned logical facts without guest
reads or construction callbacks. It checks missing nodes, active-path cycles
and known parent-shape errors even when another operand is unsupported; repeated
DAG occurrences consume actual expansion work from the shared budget.

Metadata-bearing sets project their items from the explicit set operands rather
than their nil construction placeholder. Metadata replacement preserves nil
versus an empty map and preserves metadata on the metadata map itself. Raw Form
leaves normalize reader layers once; already-logical recipe results undergo
structural validation without repeating reader normalization. Planned disposal
of replaced/unsupported trees is charged and iterative; error unwinding retains
ordinary Rust cleanup and is not a new cancellation-granularity guarantee.

The [pinned final-metadata observations](../../tests/oracle/final-metadata-recipe-observations.json)
execute custom/nil/empty top-fn metadata in the CLJS-hosted analyzer, and separately
observe runtime metadata-map identity and metadata-on-metadata preservation.
Run `scripts/probe-final-metadata-recipe.sh` for a fresh observation. Native tests
execute actual final declaration records in both phases, distinguish revision
owners, reject later mutation, and demonstrate zero projection callbacks versus
nine existing numeric hash callbacks during set construction. These observations
do not establish a transport callback policy or issue any source-storage/memo
authority. Operational storage validation, receiver registration, authenticated
memo transitions and cross-call reuse require the separate integrated checks
above; this primitive alone does not authorize them.

Compiler-source finite reads now separately verify two native construction
invariants that ordinary logical decoding omits. Symbol/keyword FQN storage must
match its namespace/name, including UTF-16 content. Enumerated source-set backing
map entries must contain nil values, and the backing map's own metadata must be
nil; enclosing set metadata remains independently decoded. Violations accumulate
an invalid-source result while remaining decoded children are checked. Ordinary
macro/display reads retain their existing behavior.

These checks reject otherwise-logically-equal forged identifiers or sets.
They do not inspect rejected opaque backing values recursively, and do not
validate unused map storage, memo origin, UID/properties, aliases or every
operational representation. Errors encountered in decoded portions still
propagate. Constructor lineage, complete operational facts and authenticated
memo writes remain separate requirements before any retained-root reuse.

An internal ordinary-constructor input witness now separates expected direct
fields from observations of the result. It captures the schema and supplied
argument references before invoking the prepared constructor once, computes
padding/truncation and duplicate-name normalization natively, and binds the
returned receiver and its field-storage array by actual reference identity.
Schema strings compare exact UTF-16 contents and canonical GC types. Special
constructors retain their normal semantics without receiving ordinary facts.
Inspection and preparation use the current operation budget and cancellation
flag; incomplete preparation publishes no witness.

These facts establish direct input/reference binding only. An input object's
contents can change while its reference continues to match. Source ownership,
recursive descendant validation, property/prototype and UID rules, and legitimate
memo-write transitions need independent evidence before reuse. The integrated
controller requires all these facts. At the isolated constructor-witness stage,
shipped bootstrap regeneration and 19 focused constructor/source/metadata
regressions passed. The [native evidence](../roadmap/evidence/m4-constructor-inputs-validated-complete-inputs-2026-10-15.tar.gz)
retains exact source, artifacts and raw outputs; the two earlier fixture-failure
archives remain separate. This is direct-fact evidence, not complete retention
authorization or a passing full prototype baseline.

Dense source-array input facts separately bind the factory-created receiver and
backing arrays to element references captured before construction. They check
length, dense slots, presence-mask values, sparse-head state, initial properties,
metadata and UID, including alias replacement that preserves logical contents.
Tests use 65 distinct references and independently inspect their indexed storage,
then corrupt each spine component and collect before validation. A descendant
mutation still matches its direct element reference: recursive source facts and
descriptor-state validation remain separate requirements.

Array and constructor owner scans now poll and charge the shared budget in
argument order. A reached foreign input fails before its own charge; resource
failure in a valid prefix can preempt a later foreign input. The regression
preserves first-foreign error precedence, exercises zero fuel and interruption
across all four ordinary/tracked builders in both phases, and verifies recovery.
The [source-array evidence](../roadmap/evidence/m4-source-array-inputs-validated-2026-10-15.tar.gz)
retains the initial failing preflight regression, final 23 focused passes,
regenerated artifacts and fingerprint inputs. These direct facts do not authorize
retention or hash memo writes.

Native scalar facts capture nil/Boolean sentinels, exact binary64 bits and UTF-16
units before allocation. They bind the returned scalar by reference and compare
its contents after GC, including NaN payloads, signed zero and unpaired surrogate
units. Mutation of a string's storage invalidates its fact without refreshing the
expectation from the guest. The [scalar evidence](../roadmap/evidence/m4-scalar-inputs-validated-recovery-2026-10-15.tar.gz)
retains the two failing host-work regressions, regenerated artifacts and27 final
focused passes. Scalar, array and ordinary-constructor facts remain independent
primitives: source-owned graph registration and authenticated memo transitions
are still required before retention.

An explicitly recording bridge pairs a captured declaration revision and native
metadata expectation with its construction records. Validation checks logical
source contents, every direct fact, and membership of the selected root and all
recorded constructor/array inputs. The selected root must be explicitly recorded;
copied input references may instead belong to the pristine seed witnesses. These checks do not short-circuit Boolean
mismatches. Input membership uses an ephemeral native reference-hash index with
reference-equality collision checks; no guest execution or guest allocation occurs
while the index is live. Each validation rebuilds it, so hashes do not survive GC.
Index allocation, comparisons and traversed inputs use the shared operation budget
and cancellation checks.

Constructor inputs mean normalized retained fields: last equal UTF-16 field names
win, excess arguments are omitted, and absent fields contain the independently
checked immutable Undefined sentinel. Array inputs retain each element reference,
including aliases. A matching immediate input reference must have its own record;
its mutable contents must separately satisfy its direct facts. Unknown external
seeds decline validation. All recorded constructions are audited conservatively,
including temporary or unreachable ones.

This input-membership audit does not establish complete physical graph closure.
Descriptor state, properties/UID, seed provenance and authenticated memo changes
still need separate validation. A post-callback snapshot or a constructor field
copied from guest storage cannot supply independent source authority. The
controller consumes recording results only together with those independent
checks; a recording alone cannot authorize retention.

The three empty collection seeds have private witnesses captured immediately after
the verified shipped bootstrap runs, before a caller can mutate the Session. The
registry is staged with canonical-method observations and published only after
all captures succeed. A shipped-authority token alone cannot permit recapture: the
initial provisioning interval and capture-once checks establish the boundary.
Recording uses these original witnesses for List.EMPTY, PersistentArrayMap.EMPTY
and PersistentHashSet.EMPTY; it never refreshes them from later bridge captures.

A witness compares exact rooted storage, including descendants and shared
descriptors, using the callback-free budgeted snapshot checker. Replacement roots
and storage mutation decline validation. Roots have private handle accounting and
old witnesses reject use across Session reset. Exact descriptor comparison is
conservative: it can reject legitimate shared-state changes and is not the final
identity-preserving memo/descriptor policy. The integrated controller must combine this result with the other source/storage checks.

Seed-input membership consults initial rooted node identities and stored I31
values, including descriptor/method/environment descendants. It never reads
current descendants to enlarge that set. Equal-content replacements and newly
attached objects do not acquire membership. A mutated initial object remains a
member by identity, but its mandatory full seed-storage check fails. Membership
therefore cannot substitute for storage validation. Fallback scans are metered;
repeated seed witnesses and scans are not yet deduplicated or optimized.

Ordinary constructor facts also bind the fresh prototype carrier by identity,
require its single entry to match the completed factory's prototype, and require
the initial UID to remain the native nil sentinel. ABI null is distinct from nil
and declines validation. The cached prototype is read after successful native
construction, without an intervening guest callback: this avoids moving lazy
prototype initialization ahead of argument allocation or constructor errors.
This is factory consistency, not independent pristine prototype/class authority;
prototype descendants and legitimate authenticated UID changes need separate
policy. Normal constructor fields remain independently derived from the inputs.
