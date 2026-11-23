# MathContext constructor evidence

This constructor checkpoint was developed separately at8b70a31f while that
head was being validated. The fresh JVM Clojure1.12.6 oracle records84 complete
constructor controls: one-argument versus explicit-rounding overloads, all modes,
String grammar, precision/range boundaries, exact and floating numeric categories,
primitive/boxed numeric call sites, diagnostics and source evaluation order.
The initial failing Suss Runtime/Macro matrix retains original outputs and
source/fixture hashes; no constructor controls are skipped or substituted.

The default Clojure macro uses the one-argument constructor. Explicit rounding
selects a different constructor and checked int coercion; BigInt, Ratio, Decimal
and Double precision values can therefore behave differently between forms.
The native macro preserves this distinction with an optional rounding argument.
Original Clojure macro provenance and its full EPL notice remain, with an updated
adaptation hash. The native UTF-16 grammar implementation is original Suss code.
The local development JDK source was inspected, not copied into shipped code.

The separate BMP oracle examines all65536 UTF-16 code units using pinned JVM
Character.digit(char,10). It finds370 decimal digits in37 contiguous ten-digit
ranges, including non-ASCII digits and excluding surrogate pairs. Its complete
output and command/source/JAR hashes are retained. This is development evidence;
there is no Java dependency in shipped code.

Constructor diagnostics also include host implementation-specific class/module
text. The recorded controls retain those strings rather than replacing failures
with success; broader source-category diagnostic behavior remains subject to
fresh controls. Passing this bounded matrix does not establish complete BigDecimal
or vendored suite acceptance.

`before-constructor-overloads` records71 mismatches per phase against84 controls;
`after-constructor-overloads` passes all84 in both phases. The six frozen gates
pass actual AOT84, every UTF-16 code unit,18 prior Decimal regression tests,
the unchanged AOT size guard (3895/form;96 extra split code bytes), Java/Node-free
bootstrap reproduction/execution, and all15 foundation commands (364 Python tests).

The additional18 primary controls cover typed/ordinary-function nonnumeric
precision diagnostics and huge BigInt, BigInteger and Ratio range errors. Every
one matches in cold standalone CLI execution. `expanded-constructor-controls`
executes all102 controls in Runtime/Macro/actual AOT and passes with frozen inputs.
The original84 regression source is retained separately; original fixtures are
unchanged. This source-qualified constructor checkpoint is integrated with a
separate raw-formatter CI repair for combined validation; it is not a final-head
CI or full workspace pass.
