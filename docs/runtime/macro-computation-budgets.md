# Compiler and macro computation budgets

Macro expansion uses compiled Suss in a Macro-phase Session. Its guest execution
and explicitly accounted native work consume the same Wasmtime Store fuel
counter. Native Rust instructions are not automatically metered by Wasmtime.
Fuel is a computation allowance, not a byte limit or a complete memory policy.

## Scope and configuration

`SessionOptions::fuel_per_operation` defaults to 10,000,000. The default has not
been raised by the cumulative-accounting changes. `Session::set_operation_fuel`
(and the corresponding CompiledMacros method) changes the allowance for the next
outer operation; it does not refill an operation already in progress.

An outer operation establishes its fuel and epoch deadline. Nested request
scopes, constructors, transformer calls and inspection helpers reuse the remaining
counter. Leaving a nested scope does not replenish it. Request guards retire on
success, returned error and panic unwinding. A later independent operation starts
with its configured allowance.

Expansion accounting includes caller provenance and quotation, explicit
arguments, materialized `&env` graphs, compiled transformer execution, nested
helpers, result realization and decoding. Source reading, selection, metadata,
syntax-quote support, IR lowering and verification also have explicit work charges on their
metered entry points. The AOT and persistent REPL paths use this same compiled
expansion machinery. Runtime execution and the isolated Macro Session retain
separate Stores and configurations.

Standalone native APIs that have no Session use finite computation allowances:

| Entry point | Default allowance |
| --- | ---: |
| Form reading and conditional selection | 10,000,000 |
| ReaderState syntax-quote expansion | 10,000,000 |
| Default ExpansionHost request | 10,000,000 |
| Standalone IR, liveness and continuation verification | 10,000,000 |
| Legacy SourceOrigin provenance convenience wrappers | 1,048,576 |

Production expansion paths pass their caller's meter to provenance operations;
they do not reset the standalone provenance allowance for every form. Native
convenience APIs can establish separate allowances at stage boundaries. A default
emitter for a host without an accounting hook has its own finite allowance rather
than debiting that host's analysis ledger; custom emitter overrides remain the
host's responsibility.

## Charging and recovery

Native charges describe work performed: visited forms/graph edges, copied or
compared text units, allocated collection entries, reader bytes, and verifier
word operations. Traversal and copies are charged before their corresponding
unbounded allocations where supported. Source-tag encoding copies already
charged large scalars in bounded chunks and polls cancellation between chunks
without charging the same bytes twice. Other native clones can perform a
precharged copy as one operation.
Provenance cache hits still charge lookup, comparison and copying work. Guest
realization invokes live methods rather than trusting cached lazy storage across
calls or callbacks.

Raw form identities are interned bottom-up from exact scalar payloads and ordered
child identities. Raw metadata, namespace presence, number bits and UTF-16 units
remain part of identity; bare spans remain separate compiler facts. Key lookup
uses fallible byte comparisons that bill inspected prefixes and poll cancellation.
Planning buffers and tree growth are precharged, including possible relocation;
tree insertion conservatively admits 64 primitive updates per ancestor plus 12
fixed updates before mutation. These are computation charges, not an additional
form-size rejection policy or a promise about process memory admission.

Before graph construction invokes reader callbacks, a cycle-checked DAG counts
logical materialization frame work in reverse postorder. Repeated reader nodes
propagate every arrival; compiler sharing barriers propagate one execution.
Duplicate dependency edges remain separate arrivals. Each executed frame costs
twice its dependency count plus one for copying dependencies, visiting edges and
finishing the frame. Checked count overflow rejects admission. The planner bills
its actual compact counting work and compares required frame work with remaining
fuel; materialization bills that work when performed. Guest constructor and
callback execution consume additional fuel. This avoids expanding every logical
occurrence merely to preflight it and does not move or omit callbacks.

Immutable native source ownership does not make the transported guest object's
reachable arrays immutable. Sharing across expansions must preserve both the
pinned captured-declaration metadata identity and the original source facts,
without trusting guest-mutated storage or suppressing live construction callbacks.
Fresh construction alone prevents mutation leakage but does not satisfy that
cross-invocation identity contract. Compiler record/revision sharing and
intentional quote aliases within one graph build remain separate requirements.

IR dominance intersections use dense bitsets and charge word operations. Runtime
field-name validation reads an immutable string length once per call. Prototype
cycle checking reuses validation only inside a callback-free traversal; later
operations validate the current storage again. Such implementation changes can
reduce actual fuel cost without changing the configured allowance, user source,
observable metadata, or validation contract. Fuel costs are not stable source
size estimates or wall-clock predictions.

Optional artifact-key construction can decline reuse after 64K admitted work
units. Mandatory IR verification still runs first, and the crossing operation
is billed to the shared budget before decline is considered. Real exhaustion or
cancellation remains an error. A declined key is discarded and verified IR is
emitted fresh, without a cache lookup or entry mutation. Produced Debug fragments
are charged in full before hashing can stop; successful hash framing remains
unchanged. This cache admission policy does not reject large source forms or
change their metadata, execution effects or operation allowance.

Native Session installation reserves `suss.compiler-integrity` for future
authenticated compiler operations. Ordinary artifacts cannot inherit that
namespace from a populated linker or claim it through source provenance.
Import checks debit the active Store budget and poll cancellation. All artifacts
are checked before allocating binding cells, executing module start functions,
or publishing staged environments.
Only the embedded bootstrap, checked against its manifest and artifact identity,
receives a private Rust authority token. Installation checks each module against
that token using its verified executable-body digest and phase; authority does
not extend to other batch members. Public artifact restoration and ordinary
evaluation receive no token. The Macro bootstrap's reserved memo hook now stages
provisional receipts only for explicitly recorded constructor watches; Runtime
bootstrap keeps ordinary setters. Weak-list scans, capacity growth/relocation,
receipt staging and validation debit the active budget and poll cancellation.
No lock spans guest execution. A store followed by exhaustion publishes no new
receipt. Complete source validation and checked metadata reuse remain separate
requirements; a protected write alone grants no retained-root authority.

Nominal construction likewise reads schema length and each outer field-name
length once during its callback-free duplicate-name normalization. It still
reads the current descriptor fields and UTF-16 units on every construction,
ignores nonstring schema slots for name matching, and uses the last matching
argument without overwriting the input argument array. No schema cache survives
the call.

Protocol lookup retains its original first-four-pair path. Later complete
four-pair groups share a bounds check; partial groups retain ordered individual
checks, followed by the existing loop after sixteen pairs. Odd table lengths
are rejected before lookup. Each call reads the current table and matching
value; duplicate keys retain first-match behavior. This changes the instruction
cost by table size and match position rather than caching a prior result.

A native charge that exceeds remaining fuel leaves the Store at zero and returns
OutOfFuel. A consumed cancellation request likewise leaves zero fuel and returns
Interrupt. Native work checks the Session's cancellation flag; guest execution
also uses epoch interruption. An idle interrupt is discarded when the next outer
operation starts. Nested entry does not clear an active request.

Failed expansion does not install a partial Runtime compilation. Recovery starts
a new operation with the same configuration. Already executed effects and failed
definition handling follow the accepted Session atomicity contract; compilation
is not a transaction over arbitrary guest objects or external effects.

## Structural validity and coverage boundaries

Finite syntax is governed by computation accounting rather than fixed form-node,
source-text or language nesting ceilings. Cycle detection, malformed layouts,
foreign ownership, signed runtime lengths, checked count arithmetic and ABI
validity remain required. Native recursive paths need stack growth or iterative
walks independently of fuel. Bounded hash-trie routing and batch sizes are
algorithm choices, not user-form rejection thresholds.

The host ledger is not yet a complete bound on all compiler activity. Wasm emission, engine module compilation, cached-Wasm validation/copying and some
native cache comparison/clone paths have work outside it. Do not infer complete
compiler resource coverage from an individual metered traversal or a passing
small fixture. Allocation admission and process OOM behavior require a separate
memory policy.

Executing regression locations include `compiled_macro_cumulative_fuel`,
`compiled_macro_forms`, `portable_hir_resources`,
`portable_module_discovery_budget`, `portable::syntax_quote::tests`,
`portable::ir::metering_tests`, `portable::origin::tests`, and
`portable::origin::form_metadata::tests`. Commands, actual outcomes and raw
failure locations are retained in the handoff; issue acceptance evidence belongs
in GitHub.

The native source-constructor argument-buffer copy charges one host-work unit
and polls cancellation for each supplied reference, including extra arguments
that the ordinary constructor subsequently ignores. The opt-in constructor-input
witness additionally charges its native schema copies, expected-field references,
UTF-16 comparisons and storage inspection to the same operation. It does not
refill the budget around preparation, construction or matching. This preparation
is not enabled for ordinary transport or retained-root reuse.

Native array and constructor builders now check input owners in argument order
within the active operation budget. Each reached valid input costs one host-work
unit and polls cancellation before scanning the next input. A reached foreign
value fails before its own charge; an exhausted or cancelled valid prefix can
stop before a later foreign argument is reached. Native values are already
materialized when this check runs. Array-buffer copies also charge and poll per
reference. The helpers establish no per-element budget refill.

The native UTF-16 scalar fill loop charges one host-work unit and polls
cancellation per copied unit, in addition to guest allocation fuel. A direct
runtime string-allocation control distinguishes that allocation cost from the
native fill: the original4096-unit case consumed4099 fuel for both paths. The
regression now requires at least4096 additional units for the fill. Native
Session invocation uses the same per-argument owner scan and per-reference
copy charging as the compiler data builders. These charges debit the existing
operation rather than establishing helper-local allowances.

Initial class/schema witnesses are staged with the private post-provision method
registry, before caller exposure. They bind all bridge constructor/descriptor
references and the private SourceArray descriptor to their original schema
storage. They exclude protocol tables: unrelated live protocol changes do not
invalidate this class/schema check. Recording checks the used class before
native construction and audits the full captured set again after callbacks;
failed checks decline recorded-source eligibility without substituting pristine
constructors into ordinary transport. Missing bootstrap authority declines.
This does not establish pristine prototype authority, descendants, authenticated
UID/memo transitions, or retained-root publication. Existing full seed snapshots
can still conservatively decline unrelated descriptor changes independently.


Prototype-origin witnesses are optional private Runtime-Wasm state. AOT and
minimal Sessions leave it off. Provisioning begins collection before shipped
core runs and seals once to the same initial 23 constructor references used by
class/schema authority. Guest artifacts cannot import the lifecycle controls.
Only inline execution of the genuine owned-prototype creation branch can issue
an entry; there is no exported registration function. The query returns a fresh
flat copy and checks the existing cache without invoking its allocating getter.

Creation stages a new entry/state before one private reference publication, then
performs the ordinary cache setter. A fuel trap can leave an orphan origin, but
querying requires the matching cached prototype and creator descriptor. If the
cache setter publishes before trapping, that prototype already has its real
factory origin. Retrying can replace the bounded constructor slot. Sealing copies
selected entries without collection links, releasing the transient head and all
unselected entries. A failed provisioning call does not expose a Session; retry
requires a fresh Session rather than reconfiguring the once-sealed registry.

The audit checks original own-storage/parent/backlink references, independent
"constructor" spelling, flags and nil UID. The default Object-prototype graph is
captured only in the private initial interval and conservatively must remain
unchanged. Constructor/descriptor backlinks use initial class/schema authority,
so unrelated source ICounted changes are not recursively frozen. Additional own
properties, changed default Object state, authenticated UID transitions, special
Error prototypes and user/custom constructors are outside this eligibility
policy. Generic direct constructor facts still work for unregistered types.
These prototype facts alone do not authorize memo writes or retained-root reuse.
All Wasm work and explicit native validation retain the existing fuel defaults.

Native form imports still validate the complete form and plan its exact raw key.
An existing recipe is selected before copying an owned form payload; a cache hit
therefore avoids a discarded deep copy, but is not constant-time validation.
UTF-16 raw-key encoding reserves the known payload capacity once, avoiding
geometric relocations. Encoding, byte comparisons, allocation and cancellation
remain charged; neither change raises an execution allowance.

Definition position extraction uses a root-metadata projection. It performs the
same full original-form comparisons and reader-expansion correspondence lookup
as full macro reader data, but copies only the selected root metadata and span.
Normalization keeps its existing prefix precedence and diagnostic locations. The
complete definition/declaration transport guards and separately exposed body
fields remain intact. Cold whole-source snapshot parsing/indexing is still
charged to the caller's operation; this projection does not move it elsewhere.

Native ordered-cache insertion stops height propagation when an AVL subtree's
height stabilizes. Before mutation, a read-only pass determines the affected
ancestors and charges32primitive operations per inspected node. The existing
64operation prepayment applies to each ancestor actually rebalanced, plus the
unchanged fixed insertion charge. Allocation prepayments and fallible key
comparisons still finish before publication. Retain uses a separate full rebuild.
This avoids actual redundant updates rather than reducing their unit charge.

Raw reader keys use a native hash index with immutable exact-byte collision
chains. Hashes select buckets only; a hit still compares every byte required to
prove equality. Key IDs follow insertion order independently of bucket order.
Hashing prepays byte reads and arithmetic in bounded chunks; collision traversal,
comparisons, ordered bucket lookup and allocation remain charged. All fallible
work finishes before publishing either a new bucket or a collision-chain entry.
Cancellation may retain reserved capacity, but cannot consume an ID or publish a
partial key. This index does not authorize guest-object or metadata reuse.

A final metadata-reuse pass that performs no rebuild has validated every retained
old record after the last construction callback. Certification reuses that
invocation-local result and still polls fuel/cancellation for each entry. Fresh
records receive full validation; any rebuild requires another complete pass.
No cross-operation validation result gains authority from this optimization.

Class coverage audits first compare each input with the next canonical name.
A positional mismatch falls back to the full name search, preserving reordered
input handling. Duplicate and complete-coverage checks and every class-schema
authority query still run, including after an earlier false audit.

Final source expectations are cached per captured source in one analysis graph,
after deferred recipes finish. Successful declines are distinct from uncomputed
slots; fuel or cancellation errors never populate the cache. Expectation
allocation, storage, lookup and publication remain charged. A fresh recording
and its pending entry can share the exact immutable expectation allocation.
Certification then performs the complete recorded-source validation once and
checks that identity before omitting a duplicate logical decode. Different
allocations still require both logical comparisons, even for the same global
and declaration revision. This identity proves equality of native predicates;
it does not certify guest data or replace live storage and authority checks.

Fresh final certification shares authority query results only in its callback-free
interval after the complete no-rebuild pass. A Session/registry-bound audit owns
all query roots and uses metered exact reference comparisons, preserving identity
across GC. Class keys include name contents, constructor presence/reference and
descriptor reference; prototype keys include every field of the native witness.
Per-record coverage, duplicate names, source/storage/input facts and ownership
checks remain independent. Completed false queries may be reused; errors and
incomplete entries are never published. The audit is dropped before fixups and
publication, and is never carried through construction or rebuild callbacks.

Authority-query lookup indexes immutable class-name hashes in a metered native
ordered map. Exact names/tags and rooted references still decide collision-chain
hits; prototype queries share bucket zero and retain full witness comparison.
Name hashing bills byte reads and arithmetic, and bucket operations, chain walks,
allocation and link publication remain charged. No GC reference hash is retained
across allocating calls. All fallible staging completes before index publication.

Recording and private-parent bridge forks share an immutable, privately owned
native core containing captured roots, class tables, constructor indices and
factory bindings. Referents remain live Wasm values and retain all existing
owner, class and authority checks. Each recording has separate mutable facts,
mutex and write domain; mode flags remain wrapper-local. Capture order/time
is unchanged. The core allocation is charged once; a fork charges the constant
Arc increment/copy work instead of allocating and copying the four containers.

Native form planning borrows immutable body nodes instead of copying an entire
composite before finding cached child recipes. Generated normalized metadata
still owns its forms and moves children through the existing work stack.
Uncached atomic payload copies remain charged; owned payloads move without
another copy. Full validation, exact raw-key planning, metadata normalization,
textual body order, recipe publication and reader-occurrence rules are unchanged.
The work stack remains the existing allocation; its backing accounting is not
improved by this ownership change.


Construction-input validation builds a fresh native open-addressed identity index
for each check. Capacity is a checked power of two at least twice the record
count; allocation bytes and each initialized slot are charged. Every probe,
full-hash comparison, wrap, selected-head read and head replacement is charged.
Equal hashes retain a separate record collision chain whose membership uses
exact rooted reference equality. The index performs no guest hashing, equality,
adapter calls or guest allocation, and never survives the validation call.
Canonical-seed membership and direct storage validation remain separate checks.
Fuel and cancellation errors discard the temporary index; they never publish a
partial certification. Optional `SUSS_INPUT_INDEX_TRACE` reports reservation,
index completion and root/edge stages. Reservation includes table initialization;
a missing completion line is partial execution, not a zero-cost stage.


Logical-source comparison decodes and validates the complete finite guest value
before comparing native forms. Small unordered maps/sets use an iterative paired
comparison with charged scalar-key scans and exact key/value comparison, including
numeric bits and metadata. Admission limits this path to expected maps/sets of at
most sixteen entries anywhere in the tree; wider inputs retain the existing
unordered key-plan comparison. This is an algorithm choice, not a rejection or
language-size limit. Unique expected keys and equal entry counts establish a
bijection; actual duplicate or missing keys cannot satisfy every expected key.
The fixed abstract tariffs are eight units per pair visit, three per pair push
plus reserve's unit, and four per eligibility visit. Text/namespace comparisons,
key scans, child pushes, allocation and relocation are charged separately. These
are conservative work units, not measurements of native machine instructions.


Raw-key bucket heads use a native growing open-addressed table over full u64
hashes. Immutable exact bytes and the existing insertion-ordered collision chains
still establish identity. New distinct hashes grow the table before half occupancy
is exceeded; existing-head hits do not grow or replace a head. Growth initializes
and rehashes into private staging, charging requested backing bytes, every old
slot inspection, probe and write. All fallible charges precede publication, and
reserved key-entry publication follows without allocation or charging. Generic
charge failures preserve the prior table and IDs. Checked native capacity/layout
overflow and the existing Vec allocation panic/OOM policy remain separate from
recoverable operation fuel; this change adds no memory-admission guarantee.
