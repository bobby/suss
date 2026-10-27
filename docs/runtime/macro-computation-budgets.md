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

Transported compiler data is immutable Clojure data (design decision
2026-10-25): built-in internals are private to `suss.core`, so a macro cannot
mutate a transported object's storage. Sharing within one graph build follows
compiler records and intentional quote aliases; identity across expansions is
unspecified beyond Clojure's guarantees.

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

Native Session installation checks every artifact, debiting the active Store
budget and polling cancellation, before allocating binding cells, executing
module start functions, or publishing staged environments.

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
that the ordinary constructor subsequently ignores. It does not refill the
budget around preparation or construction.

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

Raw reader keys use a native hash index with immutable exact-byte collision
chains. Hashes select buckets only; a hit still compares every byte required to
prove equality. Key IDs follow insertion order independently of bucket order.
Hashing prepays byte reads and arithmetic in bounded chunks; collision traversal,
comparisons, ordered bucket lookup and allocation remain charged. All fallible
work finishes before publishing either a new bucket or a collision-chain entry.
Cancellation may retain reserved capacity, but cannot consume an ID or publish a
partial key. This index does not authorize guest-object or metadata reuse.

Native form planning borrows immutable body nodes instead of copying an entire
composite before finding cached child recipes. Generated normalized metadata
still owns its forms and moves children through the existing work stack.
Uncached atomic payload copies remain charged; owned payloads move without
another copy. Full validation, exact raw-key planning, metadata normalization,
textual body order, recipe publication and reader-occurrence rules are unchanged.
The work stack remains the existing allocation; its backing accounting is not
improved by this ownership change.

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
