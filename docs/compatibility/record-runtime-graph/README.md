# Whole record runtime graph dependencies

Ten complete pinned forms are authored: comp, partial, juxt, merge, merge-with,
update, select-keys, zipmap, gensym_counter and gensym. All declaration docs,
outer/returned arities, callback order and algorithm branches remain. Patches
expand defn to def/fn and exact reader shorthands only; gensym uses the immutable
nil? macro guard and select-keys retains the pinned :cljs.core/not-found sentinel.
The counter is an ordinary upstream def, not defonce; no per-expansion reset is
introduced. Public calls remain live. Upstream EPL notices/licenses are retained
by the importer.

`sh scripts/test-record-graph-runtime-oracle.sh` exited 0: 84 independently tagged
raw pinned observations match, including ordered callback/throw traces, all
returned function arity families, nil/false captures, map presence/promotion,
sentinel collision and counter reuse. Raw observations, generated fixture and
log are retained in evidence/. The native host-decoding and sole-host captured
function GC regressions are authored but UNCOMPILED/UNEXECUTED. Bootstrap remains
unchanged/stale. Reference success does not establish Suss execution.

`python3 -B scripts/check_record_graph_runtime_provenance.py` checks the complete
ordered ten-form group and exact reconstruction from the pin. The previous four
helper verifier now anchors its group at its original positions instead of the
recipe tail, allowing additive selections while rejecting duplicates. Original
336 selections, then four helper selections, are preserved unchanged and ordered.
All previous 33 Boolean/raw helper, seven reify and fourteen iterator cases remain.

Next: execute these dependencies when the native lane is available and continue
the complete compiler/analyzer source adapters, nominal macro helper graph, actual
reify/nil-iter and record factories. This group does not implement reify or records
and does not close original #18/#19.
