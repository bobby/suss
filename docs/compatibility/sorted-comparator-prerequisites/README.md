# Whole sorted comparator prerequisites — Refs #18 and #19

This authoring checkpoint retains complete pinned `ci-reduce:1545–1567`,
`compare-symbols:1115–1125` and `compare-keywords:3462–3472` from ClojureScript
`c4295f303100bbf5afac449242d30bca1126f1a1`. Whole declaration/file/adaptation
hashes, original source ranges, the upstream notice are
packaged here with the shared [upstream license](../../../runtime/core-import/epl-v10.html). All 337 existing recipe selections remain unchanged and ordered;
three complete forms are appended. Reviews remain `:in-progress`.

`ci-reduce` retains its exact documentation, private metadata, not-native
argument metadata, both arities, empty/no-init callback, initial element seed,
explicit initial value, count-before-element order, reducer calls, early reduced
dereference and recurrence. Qualified zero?/</inc preserve immutable macro
lowering. Upstream not-native lowering invokes owner protocol methods directly,
so this adaptation retains canonical -count/-nth dispatchers at definition time
rather than adding mutable public dispatcher dependencies. Those dispatchers
still resolve protocol methods dynamically. Public reduced?/deref calls remain
live. Full malformed/native-owner boundary equivalence is not established by
this source checkpoint and needs executing validation.

Both identifier comparators retain every original stored identity, namespace
presence/absence, namespace ordering/tie and name branch and their field-read
order. Each has an original private lexical defaultCompare equivalent: qualified
binary > first, then <, with repeated left/right conversion. No public compare
recursion or mutable >/< capture is used. The Closure algorithm is re-expressed;
no Closure code was copied or relabeled as EPL source.

The closed 47-case pinned corpus covers identifier branches/UTF16/empty names,
identity bypass, namespace-before-name effects, repeated conversion and original
throws; both reduction arities, count/nth/reducer order, reduced nil/false,
original throws, live reduced?/deref and immutable macro/protocol dispatcher
behavior. Five fail-closed harness tests reject malformed/stale sources,
missing/reordered/duplicate/unknown identities, false/numeric Boolean results,
extra schema fields and duplicate JSON keys. Evidence retains actual successful
reference output and the two earlier authoring failures, with SHA256/byte receipts.

`portable_sorted_comparator_prerequisites.rs` authors two tests in both caller
phases after GC: the shared ordered corpus, plus independent raw Number decoding
for signed zero/infinity/NaN, helper results, original thrown 73 payload, exact
reducer count and session recovery. It is **UNCOMPILED/UNEXECUTED**. Existing
bootstrap artifacts are unchanged and stale; source/provenance/reference checks
are not Suss execution or acceptance evidence. No Cargo graph was started.

The entire seven-class IComparable parent registration remains deferred until
BlackNode and RedNode exist. No five-target substitute is installed. Public
identifier/vector comparison, tree classes and algorithms, complete sorted
map/set factories, iterators, printers and range operations remain required by
the full sorted lane. This checkpoint closes neither #18 nor #19 nor original M4.

Next: independent review, scheduled bootstrap regeneration and focused native
prerequisite/comparator/core-import checks when the sole Cargo lane is available;
continue the complete BlackNode/RedNode/tree dependency group and then register
all seven original IComparable targets. Required full baseline and final-head CI
remain before promotion.
