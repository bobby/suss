# Source forward declarations

Retained core source declares IndexedSeq/prim-seq/array-seq before their bodies,
and later mutually dependent list, sequence and hash helpers. The bounded
bootstrap now accepts declare statements, preserving existing values and
allowing later function definitions to resolve earlier references. It lowers
to existing initializerless definitions and retains generated :declared true
source metadata. Qualified names must belong to the current namespace, under
the same checks as def. Aliases, exclusions and core qualification use the
existing phase resolver. The expansion limit remains256 operands.

A known uninitialized source variable reads as internal undefined, matching
pinned ClojureScript. Nil? and Boolean truthiness treat it as nil-like/falsey,
while undefined? distinguishes it. Reading does not initialize the cell.
Defonce still initializes a fresh declaration, preserves bound nil/false and
skips initializer effects for an initialized value. Definitions inside skipped
initializers remain undefined until initialized themselves. Unresolved names
remain located compile errors. Calling undefined produces a language error.

Source GlobalRead checks the existing binding-bound helper, then either reads
the initialized cell or returns the existing undefined sentinel. Direct internal
ABI binding-get still raises an unbound-cell language error. There is no new
helper/global/type index, recursive layout or ABI version. The initialization
flag and existing cell identity remain intact across GC and redefinition.

The reference is ClojureScript c4295f303100bbf5afac449242d30bca1126f1a1,
core.cljc's :cljs declare macro, and compiler.cljc855–892's def emission. A
hash-bound partial review documents original Rust lowering; no upstream form
is copied. Generated licensed source retains25 files with an updated review
manifest hash. Runtime Var metadata and compiled macro execution remain pending.

Seventeen shared primary observations are independently decoded after actual
Wasm execution. Native execution first loads the generated canonical core source
for its boolean function, enters the reference fixture namespace and forces GC
between cases. Tests cover undefined reads, existing values, mutual fixed
functions, old closures/live cells, defonce, qualified names and catchable calls.
Located malformed declarations, unknown names, aliases/exclusions and compile
atomicity have separate native checks. The original regression failed on
unresolved declare; a second probe exposed the prior source unbound-read error.

```sh
CARGO_BUILD_JOBS=2 sh scripts/test-forward-declaration-oracle.sh
```

Declaration expression results remain explicitly unsupported, following the
existing initializerless-def boundary. Complete Var metadata/compiled macros,
source EmptyList/List/Cons/IndexedSeq, hashing/reduction and persistent rest/apply
remain unfinished. This import prerequisite does not close a milestone or issue.
