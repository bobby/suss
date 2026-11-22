# Source numeric overload selection

These are original regression programs and Rust lowering, checked against JVM
Clojure 1.12.6. The primary references are Compiler.getMatchingParams,
Reflector.paramArgTypeMatch and the actual Numbers/Util method signatures:

- https://github.com/clojure/clojure/blob/clojure-1.12.6/src/jvm/clojure/lang/Compiler.java
- https://github.com/clojure/clojure/blob/clojure-1.12.6/src/jvm/clojure/lang/Reflector.java
- https://github.com/clojure/clojure/blob/clojure-1.12.6/src/jvm/clojure/lang/Numbers.java
- https://github.com/clojure/clojure/blob/clojure-1.12.6/src/jvm/clojure/lang/Util.java

Clojure is EPL 1.0. No Java source is copied or shipped. The JVM and shaded ASM
are development oracles only. `overloads.clj`, `descriptors.clj` and
`descriptors.log` retain the AOT bytecode probe: mixed long/double bitwise and
Util.equiv calls invoke Reflector without consuming the local hints. A selected
(long,Object) call converts its hinted operand before evaluating the next.
Numbers.max has a mixed (long,double) overload returning Object; its result must
not be inferred as double merely because one argument is double.

`oracle.clj` asserts 41 values; `oracle-44.clj` adds unchecked increment/decrement
and boxed overflow. `before.log` records the failing equality in Runtime and AOT.
`first-source.patch`/`first.log` identify the first 41-value pass. Its equality
used a numeric helper. The subsequent source compares selected same-category
primitives raw. `unchecked-before.log` records the actual [6.5 4.5] versus [6 4]
failure in both routes before unchecked inc/dec consume primitive local hints.
`second-source.patch` identifies the 44-value source and its executable hash is
retained separately. Later source/log identities are recorded in receipts.json.

The reflective-context and unchecked-extrema oracles retain additional source
class, effect-order, signed-zero and return-boundary observations. These files
alone establish JVM behavior, not Suss execution or full numeric conformance.

`oracle-48.clj` adds the reflective-result/effect-order/return cases.
`reflective-before-source.patch` and `reflective-before.log` preserve the actual
[1 true] versus [1 false] failure and the AOT rejection of a valid double return.
The final lowering preserves Object source kinds for unresolved bitwise calls
and declines numeric hint conversion when a Boolean primitive class prevents
selection of a numeric method. Runtime, Macro and actual AOT exercise all48
values and reject all eight invalid declarations.
