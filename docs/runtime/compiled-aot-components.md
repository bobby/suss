# Portable AOT component assembly

`portable::aot::component` assembles already prepared Runtime fragments into an
executable component using the same shared runtime and compiled core/source
binding initializer as native sessions. The caller supplies a resolved WIT world
and explicit export-to-source-var mappings. This development increment supports
pure bool, u8/s8/u16/s16/u32/s32 and f32/f64 functions, scalar options, strings and string options, scalar/string lists
and void results. Parameters use direct canonical signatures with at most16
flattened fields; larger signatures receive an explicit unsupported diagnostic
before effectful source macros. Other WIT boundary shapes also receive explicit
unimplemented diagnostics. Native CLI file, namespace, project and main compilation
use this pipeline. Public compiler file/namespace/project migration and the
component-target host evaluator remain unfinished.

Functions may be freestanding world exports or members of exported interfaces.
An interface mapping uses the exact resolved world key followed by `#` and the
function name, for example `test:interfaces/math@1.2.3#calc` or `alias#calc`.
The emitted component preserves versioned package names and selected aliases,
including named aliases of an existing interface. Functions stay inside their
public interface instance; empty interfaces are preserved. Interface type exports
and external-id annotations currently return explicit unimplemented diagnostics.

The component links every fragment before a private adapter start function calls
their initializers once in supplied source order. Export wrappers load live cells
and invoke ordinary compiled closures. Multiple WIT exports may map the same
source var; their wrappers share one imported cell. Core module import names must
be unique when embedded in components, even where standalone validation would
accept duplicates. Component validation is mandatory before returning bytes.
Each source artifact is checked against the current compiler/ABI and Runtime
phase before assembly. This API requires prepared compiler catalogs; it does not
reconstruct catalogs or validate a complete dependency policy from raw Wasm.

Bool inputs map to the language's false/true values. Numeric inputs box binary64
values, promoting f32 first and preserving signed/unsigned integer meaning.
All supported integer inputs fit exactly in binary64. Integer outputs require
boxed finite integral numbers within the selected type range before conversion;
fractions, NaNs, infinities and out-of-range values throw the same boundary
language exception. Negative zero converts to integer zero. Outputs require actual
booleans or boxed numbers;
f32 output demotes the binary64 value. Incompatible values throw the shared
language exception with a boundary-specific message rather than a raw cast trap.
Pure worlds gain no hidden host imports. Void results discard the source value.

The initial executing compiler suite passed five tests. Typed component calls cover
initializer order, old function captures versus redefined live cells, independent
fresh instances, forced GC, binary64 edge bits, f32 conversion, boolean and void
results, missing/duplicate/unknown mappings, unsupported boundaries and Macro
artifact rejection. Wrong-result cases independently decode language payloads
for invalid boolean sentinels/non-i31 values and invalid numeric results.
The shared-var regression failed against the prior adapter with a duplicate
`user/shared` import; after import sharing, both exports execute successfully.
The compiled-macro CLI fixture passes: a macro in an isolated compiled phase
expands source against the shipped core catalog, and the emitted component
executes its initializer and calls without host Runtime replay, including GC.
The fixture originally had a redundant manifest unwrap and inherited the core
namespace while defining its macro in user; correcting the helper and making
(ns user) explicit preserved every effect and payload assertion. Both refreshed
bootstrap images reproduce byte-for-byte without Java, bootstrap4 and Python118
pass. Independent review found no significant scoped production defect and
added two executing regressions: an early initializer language throw independently
decodes its original17 payload instead of the later99, and mixed f32/bool/f64
arguments preserve positions and f32 promotion through both branches. Signed
NaNs remain NaN across f64-to-f32 after GC. All seven compiler AOT tests pass;
original expectations remain unchanged. Their full baseline passed1129/0/17 and
CI passed at the original reviewed head; the integration rebase changes only
handoff text and requires replacement final-head CI.

The checked small integer working increment passes all eight compiler AOT tests.
Its new typed component regression round-trips all six integer types at extrema,
including unsigned values above the signed range, forces GC, accepts negative
zero as zero, and checks exactly66 source calls. Fresh invalid-result instances
independently decode language boundary messages for bounds, fractions, subnormals,
NaNs and infinities. Existing wrong-result tests also cover nonnumeric integer
values while retaining every boolean/float assertion. A controlled prior-adapter
run fails with the original unsupported-type diagnostic, then the same regression
passes with the implementation. Initial redundant-unwrap and iterator-borrow
errors in the new test helper were corrected without relaxing assertions.
Both regenerated phase Wasm/JSON reproduce byte-for-byte without Java; bootstrap4
and Python118 pass. Affected CLI60 and compiler binding/module15 pass.
Independent review added a ninth test covering mixed scalar parameter positions,
high unsigned bits, both branches and GC. The unfiltered local baseline passed
1,134 tests with zero failures and 17 existing ignores. Final-head CI remains
required for this parent increment.
The pinned canonical engine locks a Store after a failed lifted component call;
these failure fixtures use fresh Stores and do not certify trap recovery.

The exported-interface increment passes all eleven compiler AOT tests, retaining
the nine parent tests. Its typed component calls exercise versioned names,
inline and named aliases, an empty interface, a freestanding export, shared live
cells, fresh instances and GC. The component has zero host imports. Exact mapping
errors and unsupported interface type exports are checked separately. Against the
prior adapter, the positive regression fails with the unsupported-interface
diagnostic. All four CLI source-preparation tests pass, including a compiled macro
whose emitted component exposes a versioned interface, initializes once, retains
its effect counter across calls and GC, and works in independent fresh Stores.
Both regenerated bootstrap images reproduce byte-for-byte without Java;
bootstrap4 and Python118 pass. Independent review, full baseline and final-head
CI for this interface increment remain required.

Initial interface review added a twelfth compiler test. Heterogeneous interface functions
preserve bool/u32/void signatures and shared cells; a void export redefines the
live bool function, and calls after GC observe that replacement. Exactly112
counter effects are visible through both interface and freestanding exports in
two independent Stores. A failing u8 interface result independently decodes the
language boundary payload with its exact `api#bad` mapping path. That initial twelve-test focus passed, preserving the original eleven expectations.
Further review found that named aliases lost their resolved `implements` annotation,
and function `@external-id` annotations were silently discarded. The repair preserves
`Resolve::implements_value` on named interface exports and explicitly rejects
function external IDs, including interface functions, before emitting an artifact.
Parsed binary assertions check the canonical versioned annotation independently;
typed alias calls execute against the repaired component. All thirteen compiler
AOT tests pass. Hosts loading named aliases must enable
`Config::wasm_component_model_implements(true)` with the pinned Wasmtime;
the CLI component runner and AOT test engines enable it. Both bootstrap images
were regenerated for the changed compiler fingerprint. The old full run was
cancelled for this repair and supplies no final baseline proof. A new reviewed-head
full baseline and exact final-head CI remain required.

This is original Rust and ports no upstream forms. Shared GC layout and ABI2
remain unchanged. Complete selected-WIT adapters and published artifact policy,
AOT frontend migration, evaluator retirement and scheduler/cancellation/live heap
acceptance remain open. These focused results do not certify M3 completion.

## Scalar options

A scalar option parameter uses a canonical discriminant and payload. None maps to
`[:none]`; Some(value) maps to `[:some value]`, as required by design section8.
Some(false) remains distinct from None. Nonvectors, unknown tags and wrong
lengths raise language schema exceptions. `[:some nil]` is rejected for these
scalar payload types; nested/composite options, including a payload that admits
nil, remain unsupported rather than collapsing some(nil) to none. Scalar payload conversions
share the checked numeric/boolean rules above. Optional synchronous results use
a one-byte discriminant followed by a payload aligned to1/2/4/8 bytes in a bounded
one-page canonical memory. Calls do not allocate or transfer scalar payload memory.
Canonical asynchronous completion uses flattened tag/payload parameters through
`task.return`, loading the synchronous source adapter's aligned result area.
This supports non-suspending source bodies; rooted source suspension and pending
I/O cancellation remain separate M3 work.

The original twelve actual-component regressions fail on the unchanged parent
and pass with these adapters. Independent review found and repaired an incorrect
nil/raw source mapping; a tagged-vector observation fails on the initial PR head.
The schema runs as compiled source, captures private primitive identity/array closures, keyword constants and
the vector constructor immediately after core initialization, and is privately
rooted before user initializers. User namespace shadows and subsequent core cell
redefinitions cannot replace those captured values. Shape checks use nominal
constructor identity, direct schema fields and captured private primitives,
avoiding transitive calls through mutable public count/nth/=/nil? cells. Ordinary user calls retain
their live core bindings. Additional regressions cover malformed shapes and
core function/constructor redefinition. They cover typed None/Some values, false versus none, integer
bounds/alignment, exact f64 bits, f32 signed zero, mixed component type indices,
eight optional arguments, canonical async completion and repeated calls/GC.
Invalid u8 results independently decode the shared language exception; the pinned
canonical engine locks an instance after a failed lifted call, so each failure
uses a fresh instance. Successful repeated calls and post-return are checked
separately. Existing scalar13 and async3 tests pass. Both phase bootstrap pairs
reproduce without Java/Node; bootstrap4 passes. Independent review, the exact
unfiltered full baseline and final-head CI remain required for this increment.
Composite/nested options, lists, exact64 and indirect argument lowering
remain unfinished; this is not generic WIT or complete M3 acceptance.

## Owned Unicode strings

String parameters copy canonical UTF-16 units into owned GC arrays before the
source function runs. Once all arguments are copied, their input transfer blocks
are released. Source values retained in atoms remain independent of reused linear
memory. String results require an actual source string and paired UTF-16
surrogates before output allocation; invalid values raise a decoded language
boundary exception. Embedded NUL, empty strings, combining characters and astral
characters retain their units. These checks are distinct from ordinary source
string indexing, which continues to permit lone UTF-16 surrogates.

The shared owned allocator grows memory with checked arithmetic and reuses freed
blocks. Synchronous output buffers survive canonical lifting until post-return;
non-suspending async exports release them after task-return consumes the result.
String options use `[:none]` and `[:some string]`, preserving Some(empty). None
has no payload allocation; Some copies and validates its string payload. Their
canonical result fields are tag/pointer/length with alignment preserved.

Eleven typed component regressions pass with no ignored cases. They exercise
mixed scalar/string arguments, eight strings at the16-field direct-signature
limit, rejection of larger signatures before source macro effects, fresh Stores,
retained input after GC, repeated sync/async transfers, string options and
once-only finally effects. Malformed bare results independently decode language
exceptions. Transfer memory remains capped at128KiB while cumulative payloads
exceed that capacity, establishing reuse rather than bump-only allocation.
The fixture separately bounds the existing numeric scratch memory and configured
GC heap at8MiB; Wasmtime uses the same limiter for all three storage mechanisms.
The helper asserts the artifact's numeric/canonical initial memory sizes before
applying those separate budgets. The unchanged parent rejects all eleven tests
as unsupported strings. Existing scalar13/async3/option16 tests pass after the
adapter module-index and post-return concrete-type-index corrections.

This source is original Rust, with no new upstream forms or ABI layout changes.
Bootstrap reproduction, independent review, full baseline and final-head CI are
still required for this working increment. Imports, lists, exact64, nested
composites, source suspension and the original M3 acceptance remain unfinished.


The list adapter working increment transfers lists of supported small scalar
values and strings as owned persistent vectors. A private compiled schema
captures the first Runtime core nominal identities before user initializers,
builds real vector trie layers and accepts retained `Subvec` views on output.
It checks physical array/node storage and every intermediate view's full bounds,
even when the returned outer slice is narrower, then validates every
result element before allocating output buffers. Canonical nested string buffers
are released before the outer list buffer; synchronous post-return and
non-suspending asynchronous task-return perform the same cleanup. Supported
world-local type aliases do not introduce runtime imports.

The executing ten-test suite passes without ignored or filtered tests. Evidence
includes trie lengths 0/1/31/32/33/1024/1056/1057/2048, atoms and forced GC,
subvector ranges, exact numeric bits and checked boundaries, Unicode/NUL/empty
strings, repeated sync/async transfers under an independent 128 KiB canonical
memory limit, nominal core redefinition, malformed storage/elements and
pre-macro rejection of signatures exceeding sixteen flattened fields. The
universal compiled source closure ABI currently costs 199,101,049 fuel for a
1,057-element round trip; large test calls each receive a bounded 500-million
fuel budget, separately from the unchanged memory/cleanup assertions. This is
executing development evidence, not a performance acceptance claim.

Pinned Subvec, build-subvec and subvec declarations retain their source hashes,
EPL provenance, extracted originals and explicit adaptation patches through the
strict core import pipeline. The type retains every upstream method; lazy-seq
uses the retained LazySeq constructor/thunk, JavaScript errors use typed runtime
errors, and bootstrap defn/assert/int/max operations have explicit adaptations.
These declarations remain in-progress: full subvector sequence/reduction/hash/
metadata/iterator compatibility needs separate evidence. List storage is bounded
to one million entries and nested view normalization to 64 levels with explicit
diagnostics. Nested/composite lists, exact64, resource/import adapters, source
suspension, cancellation and the original M3 acceptance gates remain unfinished.
Independent review, complete workspace baseline, bootstrap reproduction and
final-head CI are required before publishing this increment as ready.
