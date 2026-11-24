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
