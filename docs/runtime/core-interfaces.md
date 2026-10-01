# Retained core sequence and collection interfaces

The canonical generated core source imports twenty unmodified defprotocol forms
from pinned ClojureScript c4295f303100bbf5afac449242d30bca1126f1a1: ICloneable,
ICounted, IEmptyableCollection, ICollection, IIndexed, ASeq, ISeq, INext, IMeta,
IWithMeta, IEquiv, IHash, ISeqable, ISequential, IList, IStack, IReduce, IReversible, IIterable and IDrop. Recipe declaration IDs,
exact source ranges and hashes, review dependencies and method signatures are
recorded in the [import recipe](../compatibility/core-import.json), review overlay
and generated manifest. Source forms and upstream EPL notices/licenses are
retained. No source text is removed or patched for these declarations.

Before import, the executing regression failed on unresolved cljs.core/ISeqable
while loading the existing six-function core artifact. After import, four native
tests load the actual generated artifact and exercise every direct marker and
method through a nominal test adapter. Number results are independently decoded
from binary64 fields; Booleans require exact sentinel values. GC runs between
calls. A separate lifecycle test reloads the source artifact and checks canonical
suss.core/cljs.core aliases, captured method values, invalid arities and recovery.
Both -nth signatures execute with distinct results, along with clone/conj/metadata adapters,
seq identity, same/different object equivalence and nil-returning methods.

The development-only pinned ClojureScript runner certifies a separate 60-case
source corpus using strict scalar transport. Native execution checks those same
observations independently. JVM/Node are development dependencies only.

```sh
CARGO_BUILD_JOBS=2 sh scripts/test-core-interface-oracle.sh
CARGO_BUILD_JOBS=2 sh scripts/test-core-import-oracle.sh
python3 scripts/core_import.py --check
```

These are partial reviews and prerequisites for issues #16/#17. The existing
compiler protocol adapter is still a bounded bootstrap; compiled upstream macros,
complete protocol reflection, source metadata and general native extensions remain
incomplete. The fixture is an original nominal adapter, not a persistent
collection. Source List/EmptyList/Cons/IndexedSeq, sequence APIs, equality/hash
algorithms, lazy/chunked effects, vector trie boundaries and structural sharing
have no acceptance claim here. Next implement the retained source sequence types
and their storage, UTF-16 indexing and variadic rest/apply dependencies.


The added IStack/IReduce/IReversible/IIterable/IDrop declarations are used by
retained List/EmptyList/IndexedSeq. Their entire source forms remain unmodified,
including annotations and documentation. The original35 scalar observations are
unchanged;25 additions execute both -reduce signatures, nil initial state,
peek/pop, direct markers, reversal/iterator identity, drop dispatch, ordered
arguments, thrown reducer effects and captured method values after extension.
These are dispatch probes through original nominal adapters. They do not certify
reduction traversal, reduced-value early stopping, actual iterators or persistence.
A fourth native test reloads core, forces GC, checks both canonical aliases and
retained reduction dispatch, and requires language exceptions for invalid method
arities followed by successful recovery.

The overlay now has80 partial reviews/985 unassessed declarations;26 selected
forms reproduce30 artifacts. There is no new compiler/runtime helper or ABI layout.
Source canonical empty-list/static-property support and complete concrete
List/EmptyList/Cons/IndexedSeq dependencies remain the next implementation task.
