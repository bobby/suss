# Short literal field probe execution — 2026-10-17

Source: `4541f9d22255d485c57b1f1c2643d98c71df4601`.
Reviewed eight-path patch: `78a77ef880f8f5538f4296131fa5fa438ad2c9451dff23e8047c4008edac1832`.

The compiler probes a fully checked live nominal field. Explicit status distinguishes
nil/false hits from misses. On a miss it emits the original UTF16 literal and
original named-property getter. The legacy getter shares the probe and retains
its dynamic fallback. Source evaluation order, full schema validation, last duplicate
winner, late malformed errors and callback behavior remain required. No GC layout,
ABI version, allowance or numeric tariff changed.

## Terminal execution

| Focused stage | Passed | Failed | Ignored |
| --- | ---: | ---: | ---: |
| Planner cancellation/retry | 2 | 0 | 0 |
| Emitted-Wasm selection/fallback | 2 | 0 | 0 |
| Runtime probe/legacy getter | 3 | 0 | 0 |
| Runtime ABI | 86 | 0 | 0 |
| Closures | 15 | 0 | 0 |
| Primitive string access | 2 | 0 | 0 |
| CLI getters, both phases | 2 | 0 | 0 |
| Pinned bitwise wrapper (70 observations) | 1 | 0 | 0 |
| Complete CoreEnv | 0 | 1 | 0 |
| Public records/reduction | 0 | 3 | 0 |
| **Focused Rust executions** | **113** | **4** | **0** |

Every planned stage executed; unselected tests and blocked value assertions are
not passes. Both normal bootstrap runs exited0. Python provenance10 and the
complete provenance checker passed separately. No test-count union is inferred
from repeated bootstrap or separate oracle runs.

The four fuel failures remain:

- `compiled_macro_core_namespace_keeps_complete_declaration_records`: ASeq
  definition origin7111..7179 after metadata remaining969, before required values.
- `public_record_nominal_equality_and_predicate_decode_after_gc_in_both_phases`.
- `public_records_execute_jank_and_authored_contracts_with_two_nominal_classes`.
- `public_reduce_dispatch_and_source_order_match_pinned_cljs_after_gc`.

The three latter tests still fail during caller-&env transport before their
required record/reduction observations. Their later phases/assertions remain
unexecuted. Passing helper tests do not replace these contracts.

Separately, the unchanged pinned Jank map wrapper passed at its existing
**200,000,000** allowances in both sessions: all69 ordered IDs, **62 host passes,
7 guest judgments**, zero failures/skips/missing assertions. Guest judgments are
not host-decided passes. This repairs the V3/V4 regression; those failed runs
remain published at `4dbf0c8` and `f74dcc3`. Map success does not quantify total
fuel savings or prove default10M execution.

## Fresh pinned value/effect contract

CLJS `c4295f303100bbf5afac449242d30bca1126f1a1`, Clojure1.12.1, JRE21.0.8+9:
compile0 and Node0 produced four strictly tagged observations for nominal
nil/false/value, getter replacement and owner/getter order, once-only nil/false
getter returns, and thrown nil/false with ordered effects. Source SHA256:
`7bbe6c666bda9799144abc13f207a4a2a9064a768e0daa530dcd4eaf5e3e9b55`.
The independent review verified exact source IDs and tagged value/effect trees.

The original identical-source attempt was intentionally interrupted143 after a
SIGQUIT dump showed slow analysis-cache writes. Its dump is in `compile.stdout`;
stderr, receipt and intervention remain retained. It produced no observations.
The successful repeat used tmpfs and `:cache-analysis false`; it is the sole
successful reference run. The archived Rust differential test is **prepared,
outside this source commit and UNEXECUTED in this packet**.

## Commands and correspondence

Raw receipts retain complete commands, environment controls and exact selections.
The controller is `run-short-literal-keys-focused.py --execute`, with reviewed
packet `short-literal-key-probe-source-final/manifest.json` SHA256
`245d87dd02954d41e8f86d945923afcb72478cac57446fb03105ee39d9e02cbe`.
Map used `--preservation --stages map`; the focused run selected the ten stages
listed above. Both use jobs1/Rayon1/test-threads1 and inherited RUSTFLAGS.

Provenance commands:

```
python3 -m unittest discover -s scripts -p test_record_abi_provenance.py
python3 scripts/check_record_macro_provenance.py
```

All2993 final frozen inputs in both runs resolve to source4541f9d or pinned CLJS;
post-bootstrap and terminal maps are identical. Before maps may additionally use
base `f74dcc328071e05790964a24ccb0aa8312fe2410` bootstrap bytes. The archive's
`git-input-bindings.json` gives exact repository/commit/blob/hash resolution.
These public Git dependencies are required; this is not a standalone source archive.

## Raw evidence and limits

Archive: [m4-short-literal-key-probe-2026-10-17.tar.gz](m4-short-literal-key-probe-2026-10-17.tar.gz).
SHA256: `3e7f7f8c92b1d15e82d5e56522e02dce2b3a6d35b874a988ce221b44586d7f9a`. It contains78 members/77 hashed payloads: complete raw
logs, selections, six freezes, source packet, source/map/oracle independent reviews,
both oracle attempts, strict comparison, prepared Rust test and source bindings.
Generated oracle output/cache trees are explicitly excluded and listed. The
focused terminal execution/archive independent review is subsequent evidence;
the earlier source review alone is not an execution verdict.

This packet establishes neither source-child/import/graph preservation on V5 nor
a required full workspace baseline or passing final-head CI. The last exact-head
#268 CI evidence remains f156df8 and failed14 fuel cases, with225 integration
targets unexecuted in each separate run. No closure, promotion, merge or original
M4 criterion completion follows from this packet. Next validation is the shared
native oracle checks and unchanged source/import/graph preservation, while actual
caller-&env work remains the implementation target.
