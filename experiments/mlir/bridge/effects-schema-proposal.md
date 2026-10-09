# Bounded effects transport proposal (not implemented)

This proposal extends the isolated #188 experiment. Numeric `suss.mlir.bridge.v1`
and its f64 closure contract stay unchanged. This is not genuine source analysis,
an EH compatibility certificate, or executed Wasm evidence.

## Registered dialect boundary

Add a separate `!suss.value` and checked `!suss.effect_closure<(inputs) ->
!suss.value, [captures]>`. Inputs are exclusively `!suss.value`; captures are
f64, i1, or !suss.value. Exactly one dynamic result is required. Admit bounded
nesting of closure operations, not recursive effect-closure capture types in
this first slice. Closure regions have exactly one entry block, capture arguments
followed by universal arguments, and recursive explicit-capture isolation.

New operations: boolean literal; dynamic numeric add/multiply; strict equality;
binding read/write; effect closure/call; structured conditional/yield; try;
effect return; number-payload throw. Existing numeric const/add can appear in
effect bodies. Dynamic arithmetic accepts only f64/value operands, produces
value, and uses existing runtime arithmetic checks. Equality produces i1 using
IR Comparison::StrictEqual; no unchecked value-to-number or value-to-bool cast.
Conditional requires i1. Effect returns/yields accept f64/i1/value; throws require
f64. Calls and Try produce value. Try requires compiled closures of arities
0/1/0, with the handler's argument explicitly value. All three regions mandatory
initially; absent handlers/cleanup and arbitrary payloads remain unsupported.

## Strict transport

Separate schema `suss.mlir.effects.v1`, separately selected exporter mode.
Exact envelope fields: schema, cells, entry. Cells are an ordered array of
{namespace, name, initial_bits}; initial values are f64. Duplicate identities
reject. Binding operations name an existing declared cell by index. No implicit
declaration, runtime free variable, or imported user function.

Each function is {parameters, operations, terminator}; parameters are ordered
{id,type} entries, including captures before universal arguments. Each operation
has explicit id, op, result_type, and the exact fields of its variant. Types are
f64/bool/value or an effect-closure descriptor containing capture types and arity.
Functions have separate logical ID namespaces. IDs are unique even across nested
conditional arms within a function; operands must dominate their use. Conditional
arms inherit outer availability; arm definitions cannot escape except via yield.
An if operation holds condition, consequent and alternative regions, whose
terminators are yields. Closure operations hold captures and a separate function.
Try holds body/handler/cleanup IDs. Call holds callee and ordered argument IDs.
Function terminators are return or throw. No inferred source/HIR binding IDs.

Expected results/cell snapshots are independent fixture oracle files, outside
the executable schema. Export rejects every unsupported attribute, operation,
type and location-dependent semantic claim, including source-analysis attrs.
Use existing 1 MiB input bound and explicit bounded values/operations/depth/cells.

## Actual IR mapping

Maintain logical-ID -> actual ValueId and physical Type maps. IDs must never be
used directly as indexes into IR values. Literals map to Number/Bool. Reads,
writes, dynamic arithmetic, calls and Try map to Value. Closure values map to
Closure(arity); capture_types retain actual physical types. Universal arguments
are Value. MakeClosure remains the existing WasmGC emitter path.

An if splits the current block with Terminator::Branch. Arm blocks have no
parameters. Each yield becomes Jump to a fresh join block with one Value
parameter; that parameter is the logical if result. Values computed before the
split remain available. The next outer operation is appended to the join block.
Nested conditionals require a returned continuation block, not an assumed block
index. Throw maps directly to Terminator::Throw; no fabricated successful value.
Try maps to Operation::Try {regions:[body,handler,cleanup]} in exact operand order.

## Required fixture observations

Retain v1 and source-schema gates. Add success and throw/handler paths, both
conditional arms, nested joins, captured f64 closure, condition evaluation once,
ordered binding writes, cleanup on success/throw, cleanup throw overriding body
success and handler result, and an outer handler observing the override. Use a
numeric journal cell plus counter to witness order/once. Reject invalid closure
types/arity, hidden captures, undeclared cells, foreign ops/attrs, wrong branch
types, nonnumeric throw and escaping arm values. Pinned Node oracle observations
and C++ gates precede native execution; no Wasmtime success claim until execution.

Remaining: missing-handler propagation, arbitrary exception values, dynamic
binding, stackless rooted continuations, suspension/cancellation cleanup, and
full genuine HIR/source-sidecar integration. These are outside this slice.
