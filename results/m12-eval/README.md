# Compiled eval dependency

Primary contains24 actual Clojure1.12.6 observations, the complete eval form,
EPL notice, exact JAR/entry/source/stdout hashes and original expressions.
They cover form data, arithmetic, functions, real binding-cell Vars, namespace
selection, macros, recursion, laziness, dynamic math context and failure effects.
These recordings alone establish no native implementation or numeric acceptance.

clojure_eval.rs requires all24 values in both native phases and an additional
reentrant compiled-macro path. Expected values are host-parsed; independent GC
storage observations retain scalar categories/Decimal scale, with only unordered
map/set observations normalized on the host. Fixture namespaces/definitions use
the ordinary compiled script path; no eval or result is supplied locally.

run-before.py links that actual test source against exact immutable pre-eval
libraries from the frozen PR507 target. The recorder itself runs inside a bounded
scope, freezes test/fixture inputs and hashes the selected libraries before/after.
It writes no Cargo cache. A native missing-name failure is a counterexample,
never a Cargo or post-implementation pass. The full34-assertion original body,
source publication and remaining issue acceptance have separate gates.

At4196f081 the durable private-capability gate returns0, inputs unchanged.
The real import-free runtime capability/GC/identity test passes (249.851s
including build; native execution3.35s). Two borrowed-Store compiler tests pass
in both Runtime and Macro (539.219s including additional workspace-feature
builds; native execution44.13s). They invoke the real compiler while a Wasm
caller is live, share existing binding cells and compile actual nominal form
data directly into Ratio/BigInt results; retained results survive GC. Neither
test supplies an eval binding or a fabricated language result.

The original1630.162s compile101 is retained with its incomplete recorder and
posthoc unchanged-input check; it contains two test API mistakes (F64 bits and
an extra ref_eq reference), fixed before the successful gate. The new recorder
writes progress to its owned log so detached stdout cannot interrupt its final
receipt. Persistent compiler publication, user macros, real Vars, namespace/
dynamic context, public eval, the24 primary controls and original34-row body
remain required. These private tests establish no public eval acceptance.

Atadb66ae9 the private runtime capability and all three borrowed-Store compiler
tests return0 (162.507/238.981s including builds), inputs unchanged. One shared
authoritative compiler catalog/cell map now serves owned inputs and callbacks;
no metadata lock crosses compilation-instantiation or source execution. The
new test publishes a callback-created binding to later owned inputs, sees later
owned definitions in a callback, and retains a compiled function over GC and
live root redefinition in both phases. The namespace getter returns an owned
snapshot rather than borrowing through the metadata lock. Existing session,
module, observation and bootstrap behavior needs broader regression checks.
Public eval, macro authority and real Var acceptance remain unimplemented.

The broader state gate at0d7f5e9e returns0, inputs unchanged:36 session library
tests, nine inline-module macro tests, four isolated observation tests and four
original bootstrap execution tests pass. Java/Node-free double reproduction
and compiler identity verification also pass (bootstrap command277.446s).
The expected caught panic in the existing nested-budget unwind regression is
retained in its log; the test and all36 session cases pass. This establishes
existing owned-session behavior after shared metadata publication, not public
eval, macro-authority sharing, real Vars or complete numeric acceptance.

At6c864a34, the borrowed session view regression returns0 (295.633s including
build; native23.67s), inputs unchanged. Runtime/Macro callbacks reuse the
existing FormBridge and invocation/compiler methods, compile actual form data
to3/2, and retain the result across GC. Owned Session remains Send. Borrowed
views cannot reset the owner, do not refill its budget, and do not retire its
pending requests on drop; owner teardown cancels exactly once. The negative
entry verifies the existing cancellation policy: consume the request, exhaust
fuel, and reject a retry as OutOfFuel without replenishment.

Earlier compile101 receipts retain the concrete-Store scope signatures and
unchanged assertions, then the original311.053s runtime101 retains a mistaken
sticky-flag test expectation. The corrected regression checks the pre-existing
consumed-request/zero-fuel contract; production consume_host_work is unchanged.
All failures remain archived. Existing session/module/observation/bootstrap
checks must pass before publishing this interface change as a new checkpoint.
No public eval or borrowed user-macro authority is claimed.

The broader borrowed-interface gate at e403e68d returns 0 with unchanged
inputs: 37 session library tests, nine inline module macro tests, four
isolated observation tests and four original bootstrap execution tests pass.
The Java/Node-free double bootstrap and compiler identity checks also pass.
Command times are 21.942, 86.937, 4.945 and 85.704 seconds respectively.
Full receipts and logs are retained in
`results/m12-eval/after-borrowed-session-interface`. These 54 regressions
validate the borrowed interface while public eval, shared macro authority,
real Vars and complete primary/body acceptance remain required.

At f8f78da5, the borrowed analysis graph gate returns 0 with unchanged
inputs. Runtime/Macro callbacks use the ordinary expansion-context transport,
then invoke a compiled transformer and compile its real result to 3/2 across
GC, without refilling caller fuel or retiring owner requests. All 60 existing
analysis-graph regressions also pass (393.767 seconds including the build for
the callback test; 8.329 seconds for the graph target). Full receipts/logs are
under `results/m12-eval/borrowed-analysis-first-native`. Shared user-macro
authority and public eval acceptance remain required.

At 5e89daf1, borrowed Macro authority uses one authoritative shared catalog
for definitions, provenance, loading, caches and declaration transactions. The
explicit typed evaluation-capability fixture expands an existing user macro
in the active Macro Store, publishes a callback-defined macro to later owned
inputs, and retains its result across GC. The gate returns 0 with unchanged
inputs: all 13 macro-library tests (including that fixture) and nine original
inline-module tests pass. Command times are 358.885 seconds including build
for the focused fixture, 12.016 seconds for the macro library and 121.126
seconds for inline modules. Full receipts/logs are retained under
`results/m12-eval/borrowed-macro-through-evaluation-capability`.

The initial duplicate-import compile failure remains archived. The subsequent
research run also returned 0, but its observation-hook fixture violated that
hook's non-reentrant contract and is explicitly unsuitable as capability
validation. The replacement uses evaluate-form directly; observation hooks
retain their contract. Public eval, the Runtime service handle, real Vars and
complete primary/body acceptance remain required.
