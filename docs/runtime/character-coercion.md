# Character coercion

`char` produces one immutable UTF-16 code unit using the accepted one-unit
string representation. It accepts Character values represented this way and
numbers whose converted unit is within0..65535, including surrogate units.
Values above U+FFFF do not become Unicode scalar characters.

Clojure's source overloads matter. `(char -0.9)` throws, while
`((identity char) -0.9)` returns U+0000. Primitive NaN throws; Object NaN
converts to zero. Direct source Double/Float calls check before truncation;
first-class/apply/Object calls first use Number.longValue. Arbitrary precision
inputs keep exact low integer bits, without a binary64 detour or math-context
rounding. The original operand is retained in numeric range errors. Each
argument evaluates once, and source class is selected before backend unboxing.

The runtime appends cast modes16/17/18 for Object/Double/Float. The public core
function wraps the Object mode; direct calls retain their source class in the
existing late numeric-overload pass. A private allocator-free formatter uses
binary32 shortest digits for source-Float diagnostics. Existing cast modes,
ABI type layouts, source budgets and artifact limits retain their contracts.

The one-unit String/Character identification has three explicit JVM differences
in the initial controls. ADR-0003 also specifies that float returns a Double:
first-class rounded values therefore print their widened Double value in range
text. That additional difference is recorded rather than called a JVM match.
No Java Character identity or boxed Float category is fabricated.

The complete upstream core form, EPL notice, primary JAR/source hashes and
control receipts are in `results/m12-character-coercion`. Its provenance tool
checks the implementation and exact fixtures against
`docs/compatibility/bootstrap/clojure-char.json`.

The frozen `character-float-format-and-legacy-guards` run passes103 tests with
source/fixture inputs unchanged. It covers369 controls in Runtime, Macro and
real CLI AOT:365 primary matches and four explicit platform differences in
each route, plus effect order, independent retained storage after GC and the
original numeric, source/module, record and size guards. The32 additional
exact-number controls match the actual CLI; their focused Macro/AOT checks and
the complete char/EDN body audits retain separate receipts. GitHub issues hold
acceptance, full-workspace, final-head CI and review state.
