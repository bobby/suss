# Retained sequential equality

The canonical import retains pinned runtime `=` (core.cljs1342) and private
`equiv-sequential` (3151), with their original docstrings, name metadata and
algorithms. Explicit source-hash-bound defn-to-def/fn patches preserve fixed and
variadic arities; compiled upstream defn and private/runtime Var metadata remain
unfinished. The forward loader declares the mutual equality dependency. Complete
unmodified number/default IEquiv extension statements from1475/1866 join the
existing singleton/nil-count setup; their source bounds/hashes and EPL notice are
verified by scripts/sequence_provenance.py. The generated artifact selects70
forms/74 licensed files;159 partial reviews/906 unassessed remain explicit.

Actual retained List, EmptyList, Cons and IndexedSeq methods dispatch through the
source equality helpers. Sequential equality crosses those types, compares nested
values, ignores metadata, rejects nonsequential arrays/strings, uses the counted
length shortcut, and stops at the first unequal element. The public `=` identity
shortcut, nil handling and variadic loop follow the pin, including shared NaN
collection identity and non-Boolean user -equiv results. No new runtime equality
algorithm or ABI layout is introduced.

The sequence corpus preserves all75 previously certified source/expectation
objects and adds56 equality observations. Fresh131 pinned ClojureScript/Node
observations match independently decoded validated Wasm results; GC runs after
each observation. New cases cover scalars/UTF-16, NaN/signed zero, nested and
cross-type sequences, array/function identity, metadata, primitive/default and
live user protocol dispatch, operand effects, counted fast rejection, comparison
short circuiting, core-cell replacement and user namespace shadowing. Six native
tests additionally check wrong arity, arbitrary typed throws from equality,
operand effects, saved closures and GC recovery. Python80, source setup4 and
import/review checks pass. The required full workspace baseline also passes. Independent PR review/
final-head CI remain readiness gates.

The first fresh130 run failed one new provisional expectation: user `def =`
shadows the user binding without changing core equiv-sequential. Corrected that
new probe to the pinned false observation, retained it as a namespace-independence
case, and added a separate qualified core-cell replacement. The first131 runner
then terminated with a pinned TypeError because a fixed function lacks the
compiler's direct arity2 attribute. The replacement is now a multi-arity function;
final fresh131 completes and matches exactly. No earlier certified case changed.

Vectors/subvectors, lazy/chunked sequences, map entries, maps/sets/records and
other persistent types remain unported on this path. Their equality/hash,
ordering, metadata and reduction acceptance remains required. Public collection
hashing and pending hash/reduction/iterator/printing helpers remain unfinished;
loading retained methods is not acceptance of those methods. This slice neither
closes an issue nor completes M4/M7.

Commands use the shared target, build jobs2 and test threads2, without RUSTFLAGS:

```
sh scripts/test-sequence-oracle.sh
cargo test -p suss-cli --test portable_sequences --locked -- --test-threads=2
python3 -m unittest discover -s scripts -p 'test_*.py'
python3 scripts/core_import.py --check
python3 scripts/sequence_provenance.py
python3 scripts/cljs_reviews.py
cargo test --workspace --locked -- --test-threads=2
```

Next close retained sequence method dependencies: ordered/public hashing,
reduced/reduction, iterator/reversal and printing/index search, while implementing
the remaining persistent types and compiled macro path toward full acceptance.


Independent PR102 review preserved all original131 corpus objects and added six
edge observations: direct and dispatched counted length rejection, ordered count
effects, uncounted length mismatch, nil/protocol direction, variadic comparison
short circuiting, and signed-zero/NaN rest values. Fresh137 primary observations
match independently decoded native values. The original CountProbe observation
uses default identity and does not itself exercise the counted shortcut; the new
IEquiv delegation reaches that branch and proves both count effects occur in order
without calling seq. Native wrong-arity tests now assert the effect trace after
each failure, including the intermediate17, before GC recovery. No production
defect was found in the retained algorithms or complete setup statements.

Independent review Python80/import74/setup4/reviews159+906 checks pass. Final
latest-source full workspace baseline passes after tightened native effect tests,
with all required enabled suites passing and existing manual ignored/diagnostic
gaps unchanged. Review logs and commands are in the handoff. Exact reviewed-head
CI remains a separate readiness gate; PR102 review makes no M4/M7 completion claim.
