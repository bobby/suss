# Compilation fuel boundaries

This describes the compiled Macro/Runtime preparation pipeline.
Fuel is a work allowance, not a CPU-time, memory or whole-compilation cost measure.

## Request boundaries

An outer [`ExpansionRequest`](../../crates/suss-compile/src/portable/mod.rs)
opens the compilation Macro Session's operation scope. Nested expansion requests,
recursive macro calls, transport helpers and result decoding consume the same
remaining Store fuel. They do not replenish it. Only a new outer operation starts
its configured allowance; RAII guards retire scopes on success, failure and unwind.
See [`Session::budget` and operation scopes](../../crates/suss-compile/src/portable_session.rs).

`Session::eval_with_macros` opens a request before its metered reader, source
handling and preparation of the supplied fragment. Multiple macro calls within
that fragment share its compilation allowance. Evaluation of the prepared result
uses the caller Session's own operation budget.

AOT has a different initial boundary. Its
[`read_script_forms`](../../crates/suss-compile/src/portable_repl.rs) native parse
and conditional selection occur before the staged compilation budget.
`prepare_script_compiled_batch` then prepares top-level forms through logical
requests, including source macro definitions. There is **no whole-file aggregate
fuel guarantee** across those requests. Separate selected project worlds retain
separate compilation Sessions. Formatting/display can also have its own scope.

[`CompiledMacros::define` and `load_source_namespace`](../../crates/suss-compile/src/portable_macros.rs)
establish their own outer guards and charge their metered source/reader work.
When called inside an already active request, their nested guards do not refill.

## Accounted expansion work

The Macro Store allowance covers the metered preparation paths for forms,
analysis/lowering and artifact generation, together with:

- Caller provenance, quotation of `&form`, the `&env` analysis graph and explicit
  argument transport. Proven-unobserved implicit inputs may use the existing nil
  fast paths; this does not authorize dropping observable metadata or arguments.
- Guest constructors/helpers and the transformer invocation through Wasmtime fuel.
- Definition scans for `&env` and `&form` observation, including node visits,
  temporary reference storage and symbol comparisons, before replacement
  evaluation and publication. Their host debits also check cancellation.
- Expanded-result realization and decoding, including captured lazy-sequence
  realization calls on that same Store. This is not a fresh budget per lazy node.
- Explicit conservative host charges for selected visits, byte/element reads,
  copies, allocation work, comparisons and validation. Charges are work units,
  not measured nanoseconds, and do not imply every Rust operation is metered.

See [`CompiledMacros::expand`](../../crates/suss-compile/src/portable_macros.rs),
[`FormBridge` decoding](../../crates/suss-compile/src/portable_macro_data.rs) and
[`consume_host_work`](../../crates/suss-compile/src/portable_session.rs).

## Exhaustion, cancellation and publication

Host debits check the Session interrupt flag and subtract from the active Store;
Wasm execution also observes fuel and epoch interruption. A request received while
idle is cleared when the next operation starts. This is not a deadline for native
work between checks. Nested failures do not silently obtain replacement fuel.

Macro expansion failures report the source call span and the reached stage,
including invocation or expanded-result transport. Preparation failures prevent
publication of the pending caller fragment; AOT writes output only after successful
compilation. Staged macro bindings have explicit restoration paths. Already executed
Macro effects are retained, not rolled back. A later operation can recover with the
same configured allowance. These are not atomic-filesystem or general effect-rollback
promises. See [staging](../../crates/suss-compile/src/portable_repl.rs) and
[AOT preparation](../../crates/suss-compile/src/portable_aot.rs).

## Defaults, configuration and limits

Shipped evaluation and compilation defaults remain 10,000,000 each. REPL `--fuel`
sets both; explicit `--compile-fuel` overrides compilation independently of flag
order. AOT `compile --compile-fuel` configures source compilation, not subsequent
artifact execution. See [compiled REPL configuration](compiled-repl.md).

Selected semantic tests use a shared 10B compilation allowance so their complete
programs can execute. Caller Runtime allowances, dedicated exhaustion/IRQ tests
and default-fit controls are retained. This is calibration, not a performance win
or proof that every corpus fits. See [test policy and exact selection](compilation-test-fuel.md).

Native file/WIT I/O and parsing, engine JIT work and initial core/bridge setup are
not all covered by this source-request allowance. Declaration provenance is hashed
as a metered stream of the original Form/source/path bytes before publishing the
new definition; host-debit failures retain typed errors and declaration context.
Not every Rust operation has a separate debit, so this remains a performed-work
model rather than a complete native-instruction or whole-compiler bound.

Fuel also does not bound process memory, retained objects, total stack growth or
allocation failure. Decoder `stacker::maybe_grow` calls provide stack-growth
mechanics, not a whole-process stack/OOM guarantee. Checked representation/ABI,
cycle, overflow, malformed-data, provenance and foreign-owner guards remain
separate correctness requirements. Increasing fuel does not repair those failures.
