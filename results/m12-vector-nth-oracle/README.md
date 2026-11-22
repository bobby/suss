# Clojure inline nth fallbacks

The vector gate exposed two ClojureScript-derived parameter expectations that
Clojure's inline `nth` does not share. The original corpus and recordings remain
unchanged. Fresh JVM Clojure1.12.6 executes the same bodies with global `set!`
adapted to `alter-var-root`, the corresponding JVM operation; no other forms
change. `live-nth-order` returns `[[17 23] 9]`, versus the pinned901 trace.
`live-rest-next-before-nesting` returns `[[17 23] 12]`, versus pinned1233.
The first/next replacements still execute. Only `nth` has inline metadata here.
Both JVM assertions, exact adapted forms, initial Suss failures and CI failure
are retained. They are not converted into skips or unverified expectations.

`tests/oracle/function-parameter-clojure-cases.json` supplies the two explicit
JVM corrections. The test checks IDs, exact original source and exact pinned
expectations before applying them. All39 original cases still execute in
Runtime and Macro after GC; all other pinned expectations remain unchanged.

A native indexed miss now calls the owned core body through its existing link
cell when the source call is the genuine shipped `nth` definition. This makes
sequence/string/default and MapEntry reads ignore root replacement too. The
source AST still identifies the public var. Lexical shadows and unknown/user
definitions retain their ordinary calls. Core aliases are not rewritten.

The expanded vector fixture has21 fresh JVM value pairs plus2 separately named
Suss baseline controls, executed in Runtime, Macro and actual AOT:69 observations.
The four new values cover a list, a string, a missing index/default and MapEntry
under `nth` rebinding. JVM's MapEntry factory replaces the portable three-field
constructor in that development oracle. The parameter and vector targets pass
all4 tests, zero failed/ignored (`focused-correction.log`). The earlier focused
result and the compile-placement error remain archived.

Five inputs reconstruct byte-exact from5e06fede and `source.patch` using
`source-check.py`; the original pinned corpus is included to prove it did not
change. `measure.py` runs identical four-case source with the frozen f5056ffe
compiler and this correction. All32 RSS and16 perf sums plus16 warm checks pass.
Vector estimates are167.85/167.65 bytes per iteration before and below RSS
resolution after; instruction deltas fall1628.87 to448.79. MapEntry allocation
remains about136 bytes and5579 instructions; its root-rebinding result is now3,
matching JVM. The old14 observation remains qualified to its original snapshot.
Direct and closure controls stay around28 instructions and below RSS resolution.
These are allocation/instruction measurements, not quiet walltimes.

`run-gates.py` reruns all36 focused targets on this changed compiler, then
Java-free bootstrap reproduction, Binaryen133 and the reviewed suite. Inspect
its terminal status before claiming the broader gate passed. The initial
36-target run failed and did not reach those later stages. Final-head CI and
the terminal unfiltered workspace result remain separate requirements.


The frozen gate now terminates successfully: 169 tests across 36 focused
targets, zero failed/ignored, Java-free bootstrap (4 tests), Binaryen 133
(2 tests), and reviewed-suite comparison (1 test). Terminal commands and raw
logs are retained here, together with the actual suite observations. The suite
comparison preserves reviewed known failures; it is not full conformance.
Use --reconstructed-only for this historical package after later source changes.
