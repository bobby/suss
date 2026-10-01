# Collection literal lowering

The portable HIR now lowers vector, map and set expression entries through
existing array storage, descriptor construction and captured member invocation.
This is bounded compiler evidence for [M2-02 / #9](https://github.com/bobby/suss/issues/9).
It does not implement PersistentVector, PersistentArrayMap, PersistentHashMap or
PersistentHashSet and does not complete M2 or M4.

The user accepted textual evaluation order on 2026-10-01. Every set entry runs in
textual order. Every map key runs before its paired value, then the next pair,
including maps larger than eight pairs. Vector entries also run in textual order.
Large maps bind each entry to a temporary before assembling separate key/value
arrays. Factory owner and method are captured before entry effects. A thrown
entry prevents subsequent entries and factory invocation. This preserves the
[design decision](../design/suss-0.3.1.md#12-session-protocol-and-decisions).
It does not guarantee map/set iteration order.

## Reference and provenance

Constructor interfaces and thresholds were checked against pinned
`clojurescript/src/main/clojure/cljs/compiler.cljc` emit-map, emit-vector and
emit-set, lines552–630, revision `c4295f303100bbf5afac449242d30bca1126f1a1`
(1.12.134, EPL-1.0). The Rust HIR code and development fixtures are original;
no upstream collection implementation is copied or imported by this change.
The pinned source and its license remain in the upstream submodule.

Empty literals read canonical class EMPTY properties. Vectors below32 use the
constructor/EMPTY_NODE interface; larger vectors use fromArray with no-clone true.
Small maps with distinct scalar constant keys use the array-map constructor;
dynamic small maps use createAsIfByAssoc. Large maps use fromArrays after ordered
entry evaluation. Sets use constructor or createAsIfByAssoc paths. Canonical
suss.core classes must be declared; absent classes produce located diagnostics
before fragment initialization, preserving existing bindings and effects.

## Executed evidence

`tests/oracle/collection-literal-cases.json` retains the original17 probe sources
and adds2 raw binary64 order probes. Corpus schema2 records both exact Suss
`expected` and pinned `reference` observations, with a reason for every difference.
Fresh pinned JVM compiler/Node execution asserts all19 reference observations;
native execution independently decodes all19 Suss results through raw i31
booleans and one-field boxed f64 bits. **15 shared values, 4 exact deliberate
variance observations, 0 skips.** Four observations cover two variance behaviors:
set order12 versus21, and interleaved large-map entries versus keys-first emission.
The order probes use development-only constructor-interface types and static
properties, not shipped persistent collections or protocol/equality acceptance.

Commands:

```sh
CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2 sh scripts/test-collection-literal-oracle.sh
CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2 cargo test -p suss-cli --test portable_collection_literals --locked -- --test-threads=2
python3 -m unittest discover -s scripts -p 'test_collection_literal_oracle.py'
```

Native guards also exercise owner/method capture before entry mutation, thrown
map/set entries stopping later effects and construction, retained objects after
GC, source-located missing-class errors and compile-atomic binding preservation.
Harness negatives reject missing/duplicate/changed reference observations,
replacing a variance with the Suss result, unexplained differences and skip fields.

The pre-change vector literal regression failed at HIR lowering. After the order
decision, the large-map regression failed with true where the textual-order
contract requires false, before ordered temporaries were added. Initial probe
setup errors (unmunged EMPTY-NODE property, missing core import and duplicate
syntactic map keys) were corrected in fixtures; these failures are not counted
as compiler passes. The initial17-case exact comparison reported the set-order
mismatch; it was kept visible until the explicit contract decision.

## Remaining boundaries

Complete persistent collection definitions, keyword/symbol expression literals,
quoted collections, runtime literal metadata, reader duplicate-form diagnostics
and the production CLI/AOT pipeline migration remain separate work. Reader
metadata is retained in HIR; this does not establish runtime metadata behavior.
Distinct scalar constants here are nil, booleans, binary64 numbers and UTF-16
strings; this is not a full upstream analyzer constant classification. No new
core inventory declarations are implemented or excluded by these fixtures.
Full workspace, independent PR review and final-head CI gate readiness. Issue#9
stays open pending its complete criterion audit.
