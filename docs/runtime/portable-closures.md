# Portable universal closure and call lowering

The portable source pipeline now executes fixed-arity anonymous `fn`/`fn*`,
lexical captures, computed/local/live-global callees and higher-order calls.
It uses the shared ABI v1 universal `(environment, argument-array) -> Value`
signature. It does not delegate to prototype analysis/emission or migrate the
existing CLI/AOT/macros/replaying REPL. Those clients remain future integration.

## Source and HIR

The bounded `fn` bootstrap macro resolves through the same canonical core,
alias/refer/exclusion and phase environment as `let`. Runtime vars do not hide
macros; lexical locals do. Bare `fn*` is a true special form. Function parameters
retain unique identities, spans and reader metadata. They remain dynamic Value
regardless of type hints. Named/recursive signatures, multiple arities,
destructuring, pre/post conditions (including parameter-vector metadata) and variadic rest sequences are not lowered; unsupported
syntax returns located diagnostics, never an argument-array masquerading as a
portable sequence. The fn macro rejects pre/post signature metadata conservatively,
including keys whose values are nil or false; ordinary annotations and fn* metadata
remain accepted. Empty bodies return nil; duplicate parameter names retain the
pinned reference behavior, with the final binding shadowing earlier parameters.

Capture analysis computes only free lexical identities, in deterministic order.
Nested closures propagate needed outer captures without retaining unrelated
locals. A closure captures the current immutable value reference, not a name;
mutable objects themselves retain their identity. Globals remain live reads in
the body unless explicitly bound to a local before capture. Reader metadata is
not runtime metadata. Known fixed closure arities produce located compile errors;
dynamic calls use the runtime's central arity check.

This does not implement a live public core function library. Bootstrap arithmetic
still handles only statically proven Number operands, including immutable numeric
captures. Parameters/global values and call results do not acquire unchecked
numeric facts. Portable dynamic numeric coercions, first-class core arithmetic
values and broader callable protocols/collections remain work; they are not
certified by identity/selection function tests.

## Explicit IR and executable functions

HIR contains Function and Call nodes. IR MakeClosure carries a separate verified
body, capture types/operands and arity. Body entry parameters explicitly represent
captures followed by user arguments. Call operands contain the callee first,
then all arguments, each normalized exactly once in source order. Verification
checks entry shape/types, capture facts, dominance/order, closure result shape,
known arities and dynamic call results before emission. Public malformed arity
metadata fails before allocating an unbounded synthetic parameter vector.
Closure body nesting is bounded to 64. General effect/throw/catch/suspension and
source recur/tail-position analysis remain incomplete.

The backend emits real functions with the exact universal ABI type, declares
function references, constructs private GC capture/argument arrays and calls
`closure-new`/`invoke`. No operand is re-emitted, no caller-specific function
signature crosses a fragment boundary, and only used intrinsics/cells are imported.
No table, linear memory, hidden print import, new dependency or shipped Java/Node
path is introduced. Manifest/prelude and final Wasm validation still gate output.

Runtime invocation checks closure and argument-array types before casts, then
central min/max arity before calling the function reference. Wrong arity,
non-callable values and malformed internal argument arrays raise the shared
language tag with distinct rooted descriptors. Callee and argument evaluation
has already completed before these checks; argument effects are not erased when
the callee is non-callable. Dynamic exception payloads currently contain messages
and nil data/cause; call-site source annotation and source catch/exception machinery
remain open. This is not full IFn dispatch, general exception semantics or REPL
recovery, although tests catch actual tags and reuse the Store after an error.

## Evidence and remaining integration

Twelve focused tests validate/link/execute source-generated closures and independently
decode numeric bits, UTF-16 and tagged exceptions. A wrapping import trace records
computed callee evaluation before each argument and one invocation per call.
Cross-fragment tests distinguish a captured old function value from a live global
lookup after rebinding/forced GC. Negative IR tests reject malformed captures,
body entries and calls before emission. Existing 13 pipeline/10 resolution/7 ABI
tests still pass; former unsupported-call expectations now assert actual calls.

The expanded original 34-case portable corpus matches fresh pinned
ClojureScript/Node observations exactly, including binary64 rounding/signed zero,
surrogates, nested/higher-order captures, computed callees, qualified `fn`, `fn*`,
empty bodies and duplicate parameter names. The implementation/cases are original;
no upstream implementation was copied. Function syntax/resolution was checked
against pinned `cljs/core.cljc` and `cljs/analyzer.cljc` at
`c4295f303100bbf5afac449242d30bca1126f1a1`; future core ports still need EPL
notices and reproducible extraction/patch provenance.

Run `cargo test -p suss-compile --test portable_closures --test portable_pipeline
--test portable_resolution --test runtime_abi --locked -- --test-threads=2` and
`scripts/test-portable-pipeline-oracle.sh`. Full baseline/review/final-CI evidence
is in the handoff and PR. The original legacy source corpus remains 9 differential
passes / 7 exact failures / 0 skips; all 1,065 inventory declarations are still
unassessed. M2 remains incomplete. Next connect source namespace/module/definition
loading and persistent compiled clients through resolved cells; extend signatures,
checked portable core coercions, collections/dispatch/recur/exception/async IR and
compiled macros, then retire the old backend only after replacement acceptance.
