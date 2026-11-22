# Primitive local hint observations

`oracle.clj` evaluates source strings, retaining their reader metadata, against
JVM Clojure 1.12.6. All31 value and seven diagnostic rows pass. The original
15-row oracle is retained unchanged as baseline-oracle.*. `baseline.json` records the Suss compiler
identity, command and the actual failing Runtime/AOT observation. The Runtime
failure occurs before Macro execution; AOT stops at its first failed export.
The other Suss rows are not claimed as executed.

`contexts-oracle.clj` retains nine additional JVM observations. Aliases through
if/do retain the numeric class used by inline operations, while ordinary reads
still return the original object. A use-site tag overrides a declaration tag.
Binary inline operations may convert a hinted value; ordinary numeric functions,
such as mod and bit-clear, retain their original arguments. Nary comparison is a
function call: its operands run before comparison and local numeric hints do not
select a primitive overload. Nary arithmetic's early cast failure prevents later
operands' effects. These are behavior observations for implementation; they do
not establish Suss support.

The regression covers Runtime, Macro and a compiled component with WIT wrappers.
It keeps helper hints in source; only the unused WIT argument is synthesized.
No shipped Java dependency or compatibility baseline is added or changed.

`hint-conversions.json` records the expanded conversion matrix and45 focused
passes, including actual AOT execution and diagnostic rejection. The source patch
includes the new module. first.log retains an intermediate nominal-node type
failure; second.log is the intermediate15-row pass, not proof of the expanded
source. numeric-core-oracle.* retains additional JVM observations for max/min and
unchecked inc/dec. Later overload-selection evidence covers unchecked inc/dec;
max/min still require additional lowering.

Primary implementation reference: JVM Clojure1.12.6 Compiler.LocalBinding,
LocalBindingExpr and LetExpr distinguish a local's Java class from its ability to
emit a primitive and diagnose hints before loop reanalysis:
https://github.com/clojure/clojure/blob/clojure-1.12.6/src/jvm/clojure/lang/Compiler.java
Clojure is EPL1.0; the Rust lowering is original and behavior is checked by fresh
JVM execution. No Java code is shipped.

The [overload-selection records](overload-selection/README.md) expand the active
regression to48 values and eight diagnostics. Fresh JVM values match Runtime,
Macro and actual AOT; invalid declarations are rejected in each route. Selected
equality overloads convert only primitive formals and compare same-category
primitives raw. Floating-class bitwise calls and mixed numeric equality preserve
the original Objects. Reflective bitwise result classes stay source-Object; a
primitive Boolean operand suppresses numeric hint consumption, retaining later
operand effects. Unchecked inc/dec consume primitive hints without changing boxed
overflow behavior. max/min and capture-aware return/recur checks remain separate
limits; these focused observations do not establish complete numeric conformance.
