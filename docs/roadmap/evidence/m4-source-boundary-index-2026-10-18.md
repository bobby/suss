# Source-boundary capture storage and traversal

Tested source is PR268 base96bc8d70ae447d0dba1e89a3c03e97ce0a0f84bb plus the two captured Rust files and four normally regenerated bootstrap files. The native-field-table experiment is excluded. All3633 frozen tracked/new-source file hashes remained unchanged through the tests. The subsequent evidence/handoff additions do not change compiler/bootstrap inputs.

Capture aliases now store checked nonzero capture IDs in8-byte optional slots instead of16-byte optional integers on this host. Encoding precedes capture publication; reads retain capture0, holes and out-of-range misses. Boundary discovery reuses byte-generation marks, with a metered full clear before reuse after255roots. The complete initial DAG/cycle/missing/unfinished proof and subsequent captured-child, self-reference, order and dedup checks remain intact. No quota increase, tariff cut, callback suppression or schema pruning.

## Executed checks

|Selection|Rust pass|Rust fail|Ignored|
|---|---:|---:|---:|
|New capture/generation regressions|4|0|0|
|Unchanged complete source-child fixture|1|0|0|
|Unchanged core_import|17|0|0|
|Unchanged collection-source harness|1|2|0|
|Pinned CLJS value/effects plus decoder negatives|4|0|0|
|Unchanged pinned Jank map transport|1|0|0|
|Total|28|2|0|

All30selected tests executed. The source-child fixture retains five variants in both phases,220ordered trace events and the physical/memo negative tail; final validation leaves155874units in each phase. This preserves an already passing criterion and does not by itself resolve#270.

The511-root padded native comparison costs191924 versus1752494 for the byte-exact old full-clear algorithm using the SAME new compact-index helper. It isolates reset work, not historical total cost or whole macro savings. Four regressions include capture0/holes/growth, shared diamonds, two rollovers, dormant paths at roots2/257/512, cycle rejection below a captured alias and typed fuel/host-interrupt recovery at the same allowance.

Pinned CLJS c4295f303100bbf5afac449242d30bca1126f1a1: all four unchanged value/effect cases pass after GC in both phases, including nil/false getters, replacement/order, once-only calls and thrown nil/false. Three adversarial decoder tests pass separately within that four-test Rust binary.

Pinned Jank suite95d4a9112cfc5fe6629be3d14bb99080ea00af2f: the original map source and all69ordered IDs are retained. Independent host comparison reports62host passes/7guest judgments,0failure/missing/skip. Guest judgments are not host-decided passes. Existing200M Runtime/Macro allowances remain unchanged; this is not default10M evidence.

## Remaining failures and limits

The collection projection test still fails during compiled macro invocation851..882 at its existing40M allowance. The unordered literal test still fails during caller-&env transport510..531 at default10M. Later required values/effects and phases remain unexecuted/unknown for those failures. Empty-do passes. These are the same failing names/stages as the prior published focused result; no new acceptance or complete M4 claim.

Normal bootstrap generation and target-aware compiler-identity verification PASS. The initial identity check omitted the shared CARGO_TARGET_DIR and failed build-output discovery; its transcribed tool output is explicitly labeled, and the corrected command is separately recorded. It executed no Rust tests.

The exact required full baseline, cargo test --workspace --locked -- --test-threads=2, was not run for this partial draft update while focused acceptance remains failing. Require independent final-head review and passing final-head CI before promotion or closure; no merge authorized. Remaining namespace/declaration identity, metadata, record/reduction and inference gates remain separately tracked.

## Raw evidence

[m4-source-boundary-index-2026-10-18.tar.gz](m4-source-boundary-index-2026-10-18.tar.gz), SHA256 `cb3e17cb53c6c16e4b1d3984de907e568854a170731bb0e786ce224d1bd5acd2`. MANIFEST binds every payload, including source/bootstrap snapshots, raw successful and failed logs, driver argv/environment overrides, terminal receipt, map observations/comparison and frozen file hashes. Earlier combined native-field experiment remains separate at26702f5; its311finished declarations are not a production result here.
