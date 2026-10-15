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

IR dominance intersections use dense bitsets and charge word operations. Runtime
field-name validation reads an immutable string length once per call. Prototype
cycle checking reuses validation only inside a callback-free traversal; later
operations validate the current storage again. Such implementation changes can
reduce actual fuel cost without changing the configured allowance, user source,
observable metadata, or validation contract. Fuel costs are not stable source
size estimates or wall-clock predictions.

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
