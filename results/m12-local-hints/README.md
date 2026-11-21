# Primitive local hint observations

`oracle.clj` evaluates source strings, retaining their reader metadata, against
JVM Clojure 1.12.6. All15 rows pass. `baseline.json` records the Suss compiler
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
