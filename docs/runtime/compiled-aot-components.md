# Portable AOT component assembly

`portable::aot::component` assembles already prepared Runtime fragments into an
executable component using the same shared runtime and compiled core/source
binding initializer as native sessions. The caller supplies a resolved WIT world
and explicit export-to-source-var mappings. This development increment supports
pure bool, u8/s8/u16/s16/u32/s32 and f32/f64 functions, plus void
results using direct canonical
signatures (at most16 scalar parameters). Larger signatures need indirect-memory
lowering and currently fail component validation. Other WIT boundary
shapes return explicit unimplemented diagnostics. Existing CLI compile commands
still use the legacy pipeline; this API does not complete their migration.

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

Independent interface review found no significant production defect in this
bounded scope and added a twelfth compiler test. Heterogeneous interface functions
preserve bool/u32/void signatures and shared cells; a void export redefines the
live bool function, and calls after GC observe that replacement. Exactly112
counter effects are visible through both interface and freestanding exports in
two independent Stores. A failing u8 interface result independently decodes the
language boundary payload with its exact `api#bad` mapping path. All twelve
compiler tests pass; original eleven expectations remain intact. Review changes
only tests/docs, leaving compiler fingerprints and bootstrap images unchanged.
The reviewed-head full baseline and exact final-head CI remain pending.

This is original Rust and ports no upstream forms. Shared GC layout and ABI2
remain unchanged. Complete selected-WIT adapters and published artifact policy,
AOT frontend migration, evaluator retirement and scheduler/cancellation/live heap
acceptance remain open. These focused results do not certify M3 completion.
