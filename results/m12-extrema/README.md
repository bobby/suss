# Clojure numeric extrema

This original Rust lowering follows fresh JVM Clojure 1.12.6 execution and these
primary EPL 1.0 references. No Java source is copied or shipped:

- https://github.com/clojure/clojure/blob/clojure-1.12.6/src/clj/clojure/core.clj (`max`, `min`, `nary-inline`)
- https://github.com/clojure/clojure/blob/clojure-1.12.6/src/jvm/clojure/lang/Numbers.java (`max`/`min` overloads)
- https://github.com/clojure/clojure/blob/clojure-1.12.6/src/jvm/clojure/lang/Compiler.java (source classes and method parameters)

Two long source parameters select a primitive long result; two double source
parameters select Math/IEEE min/max, including signed zero. Mixed and Object
signatures return the selected argument without changing its numeric type. A
floating storage representation alone cannot select the double/double source
overload. Synthetic binary folds carry their selected result kind without
fabricating source syntax. Each pair finishes its argument conversions and
operation before evaluation reaches later operands. Aliases keep ordinary
function-call evaluation and Object overloads. Primitive Boolean classes prevent
static numeric overload selection and leave the original Objects unconverted.

Generic scalar helpers propagate the first NaN, then the second, before checking
the other operand; ties select the second original value. The imported public
functions use those helpers, so boxed calls share the behavior. The double source helper admits Math only when both runtime values are genuine
doubles. A typed-call guard miss retains the actual boxed replacement and its
numeric category; it does not coerce a replacement long merely because of an old
function declaration. Source long/long and double/double results stay raw; a mixed result
whose selected type is unknown retains its boxed representation. Generic source
operations over raw doubles keep generic NaN/tie behavior rather than Math's zero
rule. Runtime global layouts and ABI 7 are unchanged.

`baseline.json`, `baseline-regression.patch` and `before.log` retain the four
failing-before tests, with no claim about later unexecuted negative rows.
`first-source.patch`, `first.log` and the executable hash identify the first
24-value run and the five code-generation tests. Both 1,234,573-iteration long
and 1,234,575-iteration double components are inspected and then executed exactly:
no hot-path helper calls or loop-carried boxes, and correct returned values/bits.
The checked long-sum and numeric-cast allocation checks also pass in that run.

`oracle-24.*`, `oracle-25.*` and the final oracle record successive value matrices;
`diagnostics.*` asserts four JVM compile errors. `signed-zero-primary.*` verifies
the two corrected direct scalar expectations and their retained boxed-call
controls. The upstream input records and expected compiler-observation corpus are
unchanged. The two reviewed adaptations, generated core source/manifest, and
compatibility dependency/test inventory are updated together.

`defn-setup-failure.log` retains an intermediate harness failure before direct
session value evaluation. Its setup is retained in `defn-setup-definition.sus`.
The final harness uses canonical def/fn with explicit primitive arglist metadata,
as the existing primitive tests do; fresh JVM execution validates that setup too.
The numeric code is unchanged from the first passing component run. The separate
check-profile build was explicitly interrupted in favor of execution tests; its
log/exit receipt is retained and is not a passing check.

These are bounded scalar/source-overload observations. BigInt, Ratio, BigDecimal,
capture-aware primitive return/recur checks, additional escape elimination and
quiet benchmark acceptance are not established by them. Exact source/command
identities and gate limitations are recorded in receipts.json.

The final primary matrix has26 JVM-matched values and four diagnostics. A separate
Suss replacement-value case follows ADR-0004's ordinary-IFn fallback contract:
JVM IFn$D interface casts reject a function that loses its primitive signature,
whereas Suss deliberately invokes its ordinary entry. This case compares the
optimized and boxed numeric operations and is not claimed as a JVM value match.
`rebind-negative` preserves the frozen before executable's correct long/zero
results and the initial implementation's actual 5.0 versus5 and positive versus
negative infinity regression. The final helper and MaybeDouble result retain
boxed replacements; the same component probe is rerun after the correction.
The initial bootstrap/Binaryen/suite gates are retained separately and are not
relabeled as checks of that correction.
