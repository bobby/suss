# Reader identifier hash initialization

Reader/analyzer `Symbol` and `Keyword` values built by `FormBridge` start with
nil `_hash`. The live `IHash` method performs warming. Eagerly inserting the
canonical Number changes user code that reads the memo or computes only when
it is nil: a custom method returning and storing91 on a cold leaf would instead
return the prefilled Number and omit its write.

The portable reference is ClojureScript
`c4295f303100bbf5afac449242d30bca1126f1a1`, executed through its self-hosted
analyzer and actual macro `&env`. The authored development probe is
`tests/oracle/src/suss_oracle/metadata_memo_leaf.cljs`; run it with
`sh scripts/probe-metadata-memo-leaf.sh`. It verifies the clean upstream pin,
forces fresh analysis, delegates nonsubject receivers to the captured original
method, restores the method in `finally`, and prints observations before checks.
`tests/oracle/metadata-memo-leaf-observations.json` retains the reference.

Eight cases cross Symbol/Keyword, cold/explicit canonical warm, and unconditional
write/nil-aware caching. Cold cases write and return91. Warm unconditional cases
replace the canonical memo with91; warm nil-aware cases preserve that canonical
memo. Declaration17 and metadata/leaf identity remain intact. The native tests
execute Runtime and Macro caller phases, with source controls and GC, and compare
48 rows from actual macro `&env`. Native write counters instrument the custom
branch; their expected counts follow the pinned branch rather than a separate
primary counter. Native resolve-var and AST views are not claimed by this probe.

Compiled identifier constants have a separate contract. Their lowering in
`portable/hir/quotes.rs` follows pinned `cljs.compiler` emission and supplies a
precomputed hash. The bridge repair preserves that path; its regression checks
the pinned Symbol/Keyword hash values. Original Jank `name` and `namespace` declarations provide supplemental field
coverage, using independently decoded operands and the suite comparator rather
than guest equality. The initial `keyword?`/`symbol?` selection was rejected by
the existing precision-suffix reader guard: zero assertions executed and 24
remained unexecuted. Those raw failures are retained without predicate claims.

The bridge, test helpers and observations are authored code. Existing source-backed
Symbol/Keyword definitions and constant-lowering arithmetic retain their upstream
EPL-1.0 provenance. JVM/Node remain development oracles; shipped execution has no
Java dependency. This initialization contract does not certify mutable retained
storage; that is the separate contract in issue257.

The exact workspace command `cargo test --workspace --locked -- --test-threads=2`
exited successfully at `5aff6827`, with 1808 pass executions, zero failures and
22 explicit ignores. The frozen source remained unchanged. The terminal raw log,
metadata, snapshot and classification are retained in the reader baseline archive
under `docs/roadmap/evidence/`; the handoff records its digest and limitations.
The separate CI run 38931494913 passed all ten jobs at that source head, including
all 244 planned integration targets. Neither result converts ignored tests or
previously unexecuted conformance assertions into portable compatibility evidence.
