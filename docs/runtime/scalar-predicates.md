# Scalar predicates

`nil?`, `false?`, `true?`, `number?`, `string?` and `identical?` are
first-class functions in `suss.core`, with `cljs.core` aliasing the same cells.
Calls check arity and evaluate operands once, in source order; rebinding a
predicate changes later calls but not captures of the original.

- `nil?` is true of nil. `false?` and `true?` accept only the corresponding
  boolean, so neither 0 nor the empty string is a boolean.
- `number?` accepts the Clojure numeric families, including floating NaN and
  infinities ([ADR-0003](../adr/0003-clojure-numeric-tower.md)).
- Floating and exact numeric Objects, nominal values and functions use actual
  reference identity. Integral scalars and strings retain their existing
  value/UTF-16 representation rules; a long is never identical to a double.

Floating Object consumption follows the pinned Clojure source contexts.
A primitive floating local boxes separately at each Object use, while an
existing Object local retains its reference. Original function literal sites
retain their immutable Object constants across calls and GC; distinct finite
literal sites remain distinct. Thus `(identical? 1.5 1.5)` is false,
`(identical? ##NaN ##NaN)` is true for the shared symbolic constant, and
`(let [n ##NaN] (identical? n n))` is false for separate primitive boxing.
Passing a NaN Object through an ordinary function and comparing that retained
Object to itself is true. These observations are independent of physical raw
storage and numeric box-cache optimization.

Symbolic constants share across all arities of the same source function and
its compiler-created try/catch, finally and thread-binding regions. A nested
source function has a separate pool; `with-redefs` introduces such a function.
Artifact cache keys preserve this literal alias partition. Pinned controls
and actual Runtime/Macro/AOT results are recorded in
[numeric acceptance](numeric-acceptance.md).

Long and UTF-16 scalar identity retain their recorded platform choices. For
example, `(identical? 1000000000000 1000000000000)` is true in Suss even though
the JVM's separately boxed values differ; small JVM longs also depend on its
cache. Numeric value comparison uses `=` or `==` with their Clojure category
and source-overload rules. Generic Object `=` first admits the same reference;
primitive double comparison still follows IEEE equality, so NaN source contexts
must not be inferred from physical boxing.

The compiler-owned number and same-reference tests serve core and the WIT
adapter ([WIT numbers](wit-numbers.md)); public predicate rebinding cannot alter
their checks. Distinct source floating Objects have distinct SSA values, whose
heap boxes may be deferred until an Object consumer needs them. A known typed
numeric consumer can therefore keep its raw lane without merging Object
identities.

Evidence and actual failed/passing commands:
[source Object controls](../../results/m12-number-source-objects/README.md),
[literal roots](../../results/m12-numeric-literal-identity/README.md).
Tests include `portable_scalar_predicates` and the numeric source-context,
literal-identity and primitive-entry targets.
