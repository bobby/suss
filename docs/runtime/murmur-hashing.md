# Retained scalar Murmur hashing

The pinned scalar Murmur dependencies now execute as retained source: m3-seed,
m3-C1/m3-C2, m3-mix-K1, m3-mix-H1, m3-fmix, m3-hash-int, hash-long and
mix-collection-hash. The runtime zero? function is retained too. Seven fixed defn
bootstrap patches preserve original algorithms, metadata and docstrings; three
constant declarations are byte-exact. All excerpts retain upstream EPL notices
and license packaging. No new runtime/ABI primitive or native cell is introduced.

Original bounded ->/as->/zero? expansions make those source forms executable.
-> splices the previous expression into each step, preserving callee-before-operand
ordering, source spans/metadata and tail context. as-> uses separate immutable
lexical bindings for successive results; closures keep old bindings and the last
form preserves statement/tail context. zero? expands to strict primitive equality
with numeric zero. Local calls, aliases, exclusions and both phases are explicit.
Binding destructuring, full callable collections, general metadata/privacy and
complete compiled macro bootstrap remain unfinished; this is not full macro API
acceptance.

57 fresh pinned source observations match independently decoded validated native
Wasm after forced GC, preserving the original53. They cover wrapping Murmur values,
signed-zero input identity, constant mixing, threading effects/captures/shadows/tail
recur, live globals/redefs and old captured functions. Two pinned numeric warnings
for non-number zero? probes remain visible; those observed values are not a broad
non-number numeric-contract claim. Four native tests additionally check located
malformed binders/arities, compile-atomic recovery, aliases/exclusions, both phases,
redefinitions and GC. Public M2/M3/M4 acceptance remains incomplete.

Thirteen additional partial reviews bring the overlay to126 reviewed/939 unassessed.
The canonical artifact now selects40 forms and retains44 licensed files. All source,
patch, review and recipe hashes are reproduced by scripts/core_import.py. The
reviewed bitwise prerequisite has significant captured-reducer/provenance fixes;
require final-head CI for that PR and this slice before readiness.

Do not replace hash-ordered-coll/hash-unordered-coll with argument-array folds.
Their seq/first/next/hash dependencies and actual persistent collection types remain
unfinished. String hashing needs exact UTF-16 charCodeAt semantics; numeric hashing
needs the pinned bit reinterpretation rules. Source privacy/static publication and
empty persistent collections remain separate work. Next independently review this
PR, push significant findings, run the required full baseline and require exact
reviewed-head CI, then complete retained ordered hashing and persistent lists.
