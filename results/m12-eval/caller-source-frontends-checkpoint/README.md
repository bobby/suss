# Caller-owned source and frontend validation

The frozen working overlay on `ba917029` completes all six recorded commands
with actual exit 0 and source, HEAD and Binaryen unchanged. The broad targets
contain 55 source/script/module/AOT/REPL tests, all 74 public eval tests, and
120 host tests: 249 unique target executions. The two focused GC/link tests
repeat members of those broad targets and are not added to that unique count.
Receipts and exact commands are compressed alongside this file; negative
runs retain their actual failures, including obsolete observer/namespace setup,
cold-versus-resident cost measurement, and the real five-internal-handle bug.

Native eval macro definitions compile once in their caller's Store and use its
live Var status, selected roots, captured objects and nested effects. Ordinary
source declarations retain the isolated Macro Store. Session, REPL and script
preparation use the same authority; scripts still prepare every Runtime input
before initialization. Source journals commit at successful linking and release
compiler-operation exclusivity before Runtime or dependency initializers. A
synchronized test compiles on the independent Macro Store while Runtime waits,
then invokes the newly compiled macro. Compiler helper/predicate roots remain
actual GC roots without being counted as external SessionValue handles.

Original Clojure 1.12.6 source/output/JAR receipts accompany the focused caller,
namespace, REPL and script controls. Source Store isolation is an architectural
invariant, rather than a fabricated JVM equality result. Original eval/numeric
fixtures, comparison rules, count/skip policies and resource allowances remain.

The separate script-native-publication-primary/before receipts expose a further
limitation: a later prepared script environment erases names created by an
earlier Runtime eval (native exit 1 for two controls; explicit future source
redefinition returns 42 in both implementations). Their controller-state hash
was sampled while validation ran, rather than captured as immutable state;
the completed six-command receipt provides frozen source conservation. Native
function/Var constants also still reject transport to a different phase Store.
These receipts do not establish a complete eval implementation, a current exact
workspace baseline, independent final review or final published-head CI.
