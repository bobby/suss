# Retained sequence reduction and Reduced

The source import retains complete pinned IDeref720 and Reduced1511 declarations,
plus reduced1515, reduced?1520, ensure-reduced1525, unreduced1530, deref1537,
array-reduce1569, iterable?1259, iter-reduce2605, seq-reduce2582 and reduce2628.
Every source method and reduction algorithm is preserved. Explicit hash-bound
fixed/multiple-arity defn patches keep original name metadata/docstrings/arities;
full compiled defn and private/runtime Var metadata remain unfinished. Original
extractions, patch hashes, dependency reviews and EPL packaging reproduce from
c4295f303100bbf5afac449242d30bca1126f1a1. Import82/generated86/reviews172 partial/
893 unassessed remain bounded evidence, not completed compatibility.

Reduced is an ordinary retained nominal object whose IDeref method returns its
field. Public reduce preserves the upstream dispatch order: direct IReduce,
arrays, UTF-16 strings, native IReduce, IIterable, then seq fallback. Retained
List/EmptyList/Cons and IndexedSeq reduction methods now reach real source helpers.
Array reduction captures length and reads current element values. Sequence and
iterator loops unwrap exactly one Reduced layer and stop before advancing to the
next sequence item or iterator value. Empty no-start reduction calls f with zero
arguments; a singleton returns its element unchanged; empty supplied-init reduction
returns init unchanged. These distinctions include init/singleton/zero-arity
results that are themselves Reduced. No new runtime ABI/type/bootstrap-cell or
intrinsic reducer implementation is introduced.

Original bounded if-let lowering follows pinned core.cljc339: evaluate the test
once into a fresh binding, test nil/false truthiness, then create the source binding
only in the consequent. The initializer and else see outer locals. The binding
retains its reader metadata and source span; both branches preserve their caller's
tail context, while the initializer is not tail. Core aliases, exclusions, lexical
shadowing and runtime/macro phases use existing resolution. Malformed operands,
non-vector or non-pair bindings, qualified/& bindings and non-tail recur produce
located compile-atomic diagnostics. Destructuring is an explicit pending boundary,
matching existing let lowering; full upstream compiled if-let is still required.

A separate65-case corpus matches fresh pinned ClojureScript/Node and independently
decoded native Wasm. It covers empty/singleton/start/no-start cases; lists/Cons/
IndexedSeq/arrays/strings; UTF-16 units, live array values/captured length; direct/
native/iterator dispatch; reducer and call operand order; Reduced identity and
single-layer unwrapping; early termination before later effects, including a
sequence whose next throws. Native tests additionally check saved reducers and
Reduced values across GC, wrong-arity argument effects, arbitrary typed reducer
throws and recovery. The control corpus has81 exact primary/native observations,
preserving its original59 and adding22 if-let cases for scope/order/falseyness/
captures/tail recur/qualification/lexical shadowing/exception cleanup. Three native
control tests check aliases/exclusions/both phases and located compile atomicity.

Initial focused34984 failed because the standalone control harness does not load
source inc/dec. Only new candidate syntax changed to bootstrap +/-, preserving
expected values. Focus24330 then passed control3 but failed a new native number
IReduce probe: separate method declarations replace the entire native function,
so the missing init yielded NaN. Fresh pinned89149 produced the same NaN and a
method-grouping warning; this was a provisional test setup error, not a production
dispatch defect. Corrected only that new candidate to grouped arities. Fresh1985
then certifies59 exact primary/native observations. Control16870 certifies77
exact primary/native3. Latest native50993 passes reduction2/control3; Python80,
source setup4/import86/reviews172+893 and diff checks pass. Initial failures remain
recorded; no certified expectation was changed or failure hidden by a skip.

The required full workspace baseline passes. Independent PR review/fixes/exact
final-head CI remain readiness gates. Other persistent types, lazy/chunked realization and all transducer
arities, public hashing/metadata/transient lifecycle, complete iterators/reversal/
printing, generic apply and compiled macros remain unfinished. Custom iterator
execution does not certify a built-in IndexedSeqIterator or browser host iterator.
No issue/milestone closure follows from this source dependency closure.

Commands use shared target/build jobs2, --locked/--test-threads=2, no RUSTFLAGS:

```
sh scripts/test-reduction-oracle.sh
sh scripts/test-control-flow-oracle.sh
cargo test -p suss-cli --test portable_reduction --test portable_control_flow --locked -- --test-threads=2
python3 -m unittest discover -s scripts -p 'test_*.py'
python3 scripts/core_import.py --check
python3 scripts/sequence_provenance.py
python3 scripts/cljs_reviews.py
cargo test --workspace --locked -- --test-threads=2
```

Next close retained ordered/public hashing and iterator/reversal/printing/index
helpers while progressing remaining persistent types and compiled macro acceptance.

Independent PR103 review added six reduction observations and four control
observations, preserving every original59/77 source and expected value. Fresh
primary/native runs certify65 reduction and81 control cases. New cases establish
iterator hasNext/next stop order, direct IReduce precedence over IIterable,
Reduced identity for singleton/empty arrays, indexed array length capture, nested
Reduced iterator unwrapping, and nested/captured if-let scope. A focused compiler
regression checks exact source spans, reader metadata and distinct scoped binding
identities. No significant production defect was identified in this bounded slice;
full compiled macros and surrounding acceptance obligations remain unfinished.
