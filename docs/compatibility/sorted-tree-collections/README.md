# Complete sorted tree collections — authoring checkpoint

The authoring record below is historical. Subsequent locked workspace
validation 74237 executes `portable_sorted_tree_collections` (six passed,
including three shared decoder controls) and `portable_sorted_tree_nodes`
(seven passed). The collection observation test compares all 142 pinned cases
in both phases after GC. The exact tested tree, source/log hashes, results and
limitations are recorded in [the execution receipt](evidence/validation-74237.json).
Earlier authoring and failed-reference receipts retain their original state.

This group retains 35 complete pinned ClojureScript declarations at `c4295f303100bbf5afac449242d30bca1126f1a1`. It appends after the rebased 370-form portable import base without changing those forms or extracted bytes. The tree group remains exactly 35 forms; the combined M4 import now has 406 selected forms and 410 generated importer files, including the separately reviewed portable `seqable?` dependency. All new reviews remain in progress. Source authoring is not native acceptance or completion of #18/#19/M4.

Whole forms include tree-map-add/append/remove/replace, TreeMapSeq/cursor construction, ValSeq/vals, TreeMap/TreeSet with every field/protocol/method/arity, all four public factories, take-while with its transducer arity, mk-bound-fn, and both arities of subseq/rsubseq. The public iterator graph includes complete StringIter/ArrayIter/SeqIter, independent INIT/START sentinels, nil-iter's genuine reify, iter dispatch and all three ES6 iterator types/helpers. Existing complete BlackNode/RedNode, balancing/reduction, comparator prerequisites and seven-target IComparable registration remain unchanged.

`provenance.json` binds complete originals, source ranges, hashes and adaptations. `dependency-ledger.json` records exact class fields, protocol markers and method arguments, live dependencies and pending prerequisites. Fifteen complete macro originals and seven full anonymous parents are retained; the shared upstream EPL text and notice remain intact. The two complete TreeMap/TreeSet EMPTY forms are installed in the loader in upstream order; the comparator is captured once, not rebound for every operation. ES6 iterable parents and the full IPrintWithWriter parent are retained but not falsely claimed installed.

The adaptation checker enumerates each whole-form expansion. It preserves recursive rotations/deletion priority, original found-array writes and stored-key replacement, no-op owner identity, original throw behavior, lazy range traversal and actual predicate-function identity membership. Typed calls proven by pinned compiler output use the bounded canonical method adapter; untyped ret/e/tree-map calls, sequence/chunk/destructuring helpers, predicates and bound comparators remain live. The adapter adds IEquiv/-equiv, ILookup/-lookup, IAssociative/-assoc, ISeq/-first, INext/-next and IMapEntry/-val to the previous two pairs, retaining canonical protocol/arity checks and receiver/method capture before argument effects.

Caching-hash reads once, tests nil rather than truthiness, calls the live hash function once and writes only after success. The complete doseq expansion keeps both chunked and nonchunked branches and the original TreeSet pair-destructuring callback behavior. No public comparison recursion or five-target IComparable substitute is introduced.

## Evidence and pending gates

The 142-case corpus returns actual scalars/vectors/Error values and ordered traces, without wrapping outputs in guest equality assertions. The pinned host transport reads actual vector trie/tail storage and exact number bits. Cases cover constructors and insertion orders, odd/duplicate arguments, equivalent stored keys, nil/false values, deletion/no-op lookup, both comparator conventions, cursor/value sequences, both range arities and predicate identity, lazy/transducer take-while, mutable iterator advancement/exhaustion, ES6 returned value/done records, upstream forEach behavior, metadata, cache/throw behavior, typed Error versus scalar rethrow, live delegates and expression order. Fresh pinned compile/Node matched all 142 expectations.

The first 126-case run failed because the authored expected scalar TreeSet forEach message omitted upstream's Number constructor text. Its log, cases and observations are retained separately. The expectation was corrected to the exact observed message, with no wildcard or allowance. That constructor-text portability remains an explicit pending native boundary; the native test requires the exact message. The expanded final reference run passed 142 cases.

Three new native regressions are authored **UNCOMPILED/UNEXECUTED**: exact both-phase raw values/traces/errors after GC; direct source-schema child sharing, comparator/owner identity and original scalar throw; all additional typed protocol pairs under public rebinding plus method-capture ordering and invalid signatures. They use the independent portable decoder/canonical Error storage, not guest printing/equality. The prior 86 supplementary tree assertions and 52 independent tree-node observations remain unchanged. The independent `883ef0e` repair review resolved both prior P1/P2 findings at authored-source level only.

Commands and retained logs:

- `sh scripts/test-sorted-tree-oracle.sh`: 142 fresh pinned raw observations passed.
- `python3 -B -m unittest discover -s scripts`: 258 passed in 11.822s; malformed kind/bits/shape/order/mode/count/message/UTF16/depth controls included.
- `python3 -B scripts/check_sorted_tree_provenance.py`: 35 forms, every field/marker/method/arity, seven anonymous parents, fifteen macros, loader, EPL and all 370 rebased-base selections/extracts verified.
- `python3 -B scripts/check_tree_node_provenance.py`: previous source/receipts and new raw receipts verified; node append check scopes the seven entries at offsets 363–369 so subsequent complete groups can follow.
- `python3 -B scripts/core_import.py --check`: 410 files verified.
- `python3 -B scripts/cljs_reviews.py`: 483 reviewed / 582 unassessed; group still in progress.
- Rustfmt parse/check of the authored test and `git diff --check`: passed, not compilation.

`evidence/receipt.json` records reference/input/retained-output hashes. Development generated JS hashes record this run but generated outputs are not shipped/committed. All four bootstrap artifacts remain byte-identical to `0c98b4f` and stale for this source. No Cargo/bootstrap/push occurred; baseline96451 owns the coordinated native lane.

Native readiness still requires actual nil-iterator reify support, genuine js/Error ancestry/binding for the retained typed catch, complete ES6 registration and complete printer target dependencies. Those forms are not excised or replaced with restricted substitutes. Regenerate and independently reproduce bootstrap only when coordinated, then compile/run focused source/compiler/collection gates, the unchanged full baseline and exact final-head CI. Full persistent record/equality/hash/transient/host/prototype acceptance remains open.
