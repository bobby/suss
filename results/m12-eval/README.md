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
