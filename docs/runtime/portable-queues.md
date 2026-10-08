# Portable persistent queues — partial M4 / #18

Contract: pinned ClojureScript c4295f303100bbf5afac449242d30bca1126f1a1,
not JVM queue semantics. This lane retains complete PersistentQueueIter,
PersistentQueueSeq and PersistentQueue declarations. The only iterator source
adaptation replaces two JS Error allocations with the existing owned portable
Error constructor. `next` still throws on exhaustion; `remove` still returns
the Error value. First `hasNext` may return the front sequence rather than a
Boolean; tests preserve this identity and normalize truthiness separately.

[Source/provenance](../compatibility/queue-foundations/provenance.json) and
[notice](../compatibility/queue-foundations/NOTICE) retain exact pin, form ranges,
hashes and upstream EPL terms. The importer reproduces329 artifacts, the overlay
validates426 reviewed/639 unassessed entries, and setup provenance validates23
licensed full forms. The canonical EMPTY initializer and Queue/QueueSeq printer
extensions are pinned complete source stanzas; loader/patch hashes are updated.
These counts establish reproducible bytes, not native compatibility.

The [shared corpus](../../tests/oracle/queue-cases.json) preserves23 original
cases and adds18: nil/false elements, nil/false metadata, rear-vector trie
boundaries at65, transfer/pop/regrowth and queue-sequence metadata. Fresh
`sh scripts/test-queue-oracle.sh` passes41 values and11 ordered iterator
observations against pinned CLJS. Three focused Python tests reject missing,
reordered, mistyped, duplicate/nonfinite/oversized and trailing input. The corpus
is not accepted merely because the expected predicates are true: native tests
must independently decode every Boolean ABI result and compare all IDs.

[Native regressions](../../crates/suss-cli/tests/portable_queues.rs) run both
Runtime and Macro phases, force GC, inspect queue descriptor/front/rear sharing,
decode count fields, and keep the original queue alive solely through its host
handle after clearing fixture globals and dropping the storage tuple. Retained
contents are independently decoded through FormBridge after projection into a
vector. Iterator tests cover ordered values, exhaustion and the exact returned
versus thrown Error descriptor/message distinction. The source-array tuple is
decoded through nominal fields/backing storage, following the current ABI.

Current native status: **3 passed / 0 failed / 0 ignored / 0 filtered**, 6.16s,
covering both phases. The first run failed on missing public clone, and the next
failed on missing public last. Both complete pinned forms are now retained with
explicit def/fn bootstrap only; last's nil? macro is qualified through the
immutable bootstrap predicate. Six added pinned cases verify empty/false last,
public predicate redefinition, once-only live next, live clone dispatch and
argument-before-dispatch effects. No queue-only shortcut or removed assertion.
Bootstrap artifacts reproduce byte-identically twice with Java/Node absent;
identity invalidation and all four compiled bootstrap tests pass. All231 Python
tests pass. Full workspace baseline and green final-head CI remain required. Full #18 still includes sorted collections and records;
this lane closes none of those criteria and makes no M4 completion claim.
