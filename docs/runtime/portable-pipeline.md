# Portable compiler bootstrap

`suss_compile::portable::compile(source)` now generates an executable core
fragment from lossless reader forms. It does not convert forms through EDN or
call the old analyzer/emitter. The existing CLI, component compiler, macro
evaluator and source-replaying REPL have not migrated; this is a replacement
pipeline under construction, not a completed second production compiler.

The currently executing boundary is:

```
source -> conditional selection -> lexical/bootstrap resolution + HIR
       -> explicit control-flow IR -> verification -> shared-ABI Wasm
       -> ABI/prelude verification + Wasm validation
```

Namespace loading and compiled macro expansion remain integration work. Bootstrap
`let` syntax is handled directly until source-backed macro expansion is available.
Arithmetic bootstrap bindings resolve to intrinsic identities; `suss.core` and
`cljs.core` qualified arithmetic calls select the same identity. This is not full
namespace/phase resolution or live core binding cells. Lexical bindings shadow
ordinary callables and the bootstrap `let` macro, so a local `+` never silently
calls the global intrinsic and a local `let` never invokes binding syntax.
The actual `if` and `do` special forms remain unshadowable.

## HIR and IR contracts

HIR retains byte spans, ordered reader metadata, inferred scalar information and
unique lexical binding identities, including binding-name metadata. Supported
forms are nil/booleans, ordinary binary64 numbers, lossless UTF-16 strings,
lexical `let`, `do`, `if` and statically verified numeric `+`, `-`, `*`, `/` calls.
Numeric zero and empty strings are truthy. Missing `if` alternatives and empty
bodies yield nil. Wrong arity, unsupported forms and unresolved names return
located diagnostics. Dynamic numeric checking/coercion is not implemented;
operands that cannot be proven Number are rejected rather than passed to an
unchecked runtime cast. No complete upstream arithmetic inventory item is claimed.

IR contains typed value IDs, ordered instructions, explicit blocks, branch/jump
terminators and edge parameters. Every operand is evaluated to a value before
numeric calls or truthiness inspection. The emitter receives value IDs and
cannot re-emit source expressions to inspect a type. Branches evaluate only the
selected block. All replacement values are pushed before assigning any edge
parameter; an executed backedge swap regression distinguishes this from sequential
replacement. Source-level loop/recur and tail-position checking are still open.

Verification checks entry shape, reachability, edge targets, unique/complete
value definitions, dominance/order, edge arities/types and resolved intrinsic
arities/Number operands. Malformed HIR binding identities and arithmetic arities
produce diagnostics.
Direct HIR Negate retains unary negation, including the sign of zero.
`compile_ir` runs verification before emission and validates the final Wasm.
The closed instruction set currently admits constants and allocating numeric
intrinsics; general calls, throws, suspension and effect analysis are future IR
extensions, not certified by these restricted tests.

## Generated artifacts and executing evidence

Fragments begin with the exact shared recursive ABI v1 group and its compiler/
ABI/tool versions manifest. They import only used intrinsics from `suss.runtime`,
export parameterless `eval -> Value`, and have no hidden print/WASI imports or
canonical memory. Numbers/strings are constructed with the runtime's verified
storage helpers; no legacy integer tags or UTF-8 values appear. The current
backend dispatches explicit blocks through a program-counter local. It is not an
optimizing backend. GC locals/host roots retain values across independently
compiled fragments in one Store; this is not a persistent compiled session.

Thirteen focused tests validate/link/execute actual artifacts and independently
inspect the heap after forced GC. The original 14 scalar reader cases now also
execute through compiled source fragments. A separate original 20-case source
corpus matches freshly compiled pinned ClojureScript/Node observations exactly,
covering numeric bits/arities, conditional values, lexical shadowing and strings.
A wrapping runtime import records once-only ordered calls and proves unselected
arithmetic does not execute. Adversarial IR tests reject malformed definitions,
dominance, types and edge/call arities before an artifact can be emitted.

Run `scripts/test-portable-pipeline-oracle.sh` for fresh reference observations
and Rust execution. The generated reference fixture is `.cljc` because the
pinned compiler rejects reader conditionals in `.cljs`; the initial wrong fixture
failed and was corrected, not skipped. Strict transport gates reject wrong pins,
missing/duplicate/changed results, boolean/integer confusion and extra/trailing
data. JVM/Node are development-only. The implementation and cases are original;
no upstream core code was copied. Semantics were checked against pinned
ClojureScript `c4295f303100bbf5afac449242d30bca1126f1a1`, including its EPL-1.0
core macro source for optional `let` bodies. Future imported core forms still
require extraction/patch hashes and retained licenses.

These bounded observations do not replace the existing 16-case legacy compiler
corpus: it remains **9 differential passes, 7 exact failures, 0 skips**. No
expected failures changed. M2-01/02/03 remain incomplete. Next, connect namespace/
phase resolution and binding cells, then universal closure/callee lowering,
dynamic checks and the source corpus; add collection/dispatch/recur/exception/
async IR forms and migrate AOT/REPL/macros through the same pipeline. Retire the
old backend only when replacement acceptance passes.
