# Analyzer literal provenance and entry-buffer metering

The source tested here is base `c170e116216ba26d3cb9b48b7c778ae6f185d4c8`
plus seven reviewed Rust files and four normally regenerated bootstrap outputs.
The source manifest SHA256 is
`280439e6551bc7b5901b7f6a2789500e7e4a61fae8b1a1875f62d6dcbfd28224`.

Explicit analyzer constructor positions initialize keyword hash memos according
to pinned ClojureScript `c4295f303100bbf5afac449242d30bca1126f1a1`.
Reader values, dynamic keys and Suss extension keys remain cold. Separate key
tables preserve this distinction even for equal text. Live collection hash
dispatch remains observable; initial numeric memos do not authorize protected
originally-nil transitions. Larger native field-entry buffers prepay their added
allocation, initialization and growth work before evaluating entry expressions.
The archive preserves the initial metering finding and its reviewed correction.

## Executed checks

Bootstrap generation and identity verification pass. Every planned focused
stage executed, with **29 passing tests, 3 fuel failures and no ignored tests**:

| Selection | Passed | Failed |
| --- | ---: | ---: |
| New native provenance, entry-buffer and streamed-hash tests | 8 | 0 |
| New compiled-macro literal observation tests | 2 | 0 |
| Existing private-slot and reuse guards | 17 | 0 |
| Original source-tag tests | 1 | 1 |
| Original complete core environment | 0 | 1 |
| Original collection-source test | 0 | 1 |
| Original pinned-map wrapper | 1 | 0 |

The unchanged invocation-tag regression now passes in both Runtime and Macro
caller phases, including its original value assertions after GC. The remaining
failures are inference during caller `&env` transport at382..414, core
environment during caller `&env` transport at0..33, and collection source
during macro invocation at851..882. Limits, tariffs, fixtures and retained
metadata were not reduced.

The independent pinned-map comparison verifies all69 ordered assertion IDs:
62 host passes, 7 guest judgments, no failures, skips or unexecuted assertions.
Guest judgments are not independent host passes.

All3542 input paths were frozen before the first Cargo command, including the
three new source files. Only the four normal bootstrap outputs changed during
generation; all subsequent inputs stayed fixed. The controller is terminal and
its process group drained. The archive includes commands, environment overrides,
raw logs, before/after snapshots, source provenance and independent reviews.
Failure/cancellation samples do not establish exhaustive whole-compiler coverage.

The full workspace command was not repeated while focused acceptance remains
red. `cargo test --workspace --locked -- --test-threads=2` and passing final-head
CI remain acceptance gates. Current Cargo metadata lists252 integration targets;
this focused run is not evidence for all of them.

## Separate preceding measurements

[CI run39162616532](https://github.com/bobby/suss/actions/runs/39162616532)
tests the preceding c170 source: foundation passes; integration has124 passes
and18 fuel failures, with31/251 targets completed and220 unexecuted. Its
failure names match the preceding4f run. It does not validate the new source.

The separate c170 diagnostic with a2-billion-unit Macro allowance completes
the first expansion at1,701,286,018 units,91 units more than4f. The second
expansion still fails constructing `&env` at0..36; the whole test fails and
the Macro caller phase is unexecuted. Complete compilation cost remains
unknown. That diagnostic changes no default and is separate from the current
focused run. Its temporary test change was restored.

## Archives

- [Focused literal and entry-buffer evidence](m4-pr256-literal-field-entry-focused-2026-10-18.tar.gz), SHA256 `8bc19828aca5c844b669672db8cd234e35ebf2ba0061b8b6f59fe79cf8b16cae`.
- [Separate c170 2B diagnostic](m4-pr256-coreenv-2b-c170-diagnostic-2026-10-18.tar.gz), SHA256 `467ce857796e754568c5cab62f0378a808a9d1dab5f2e5a2bd71370763d631f6`.

This evidence establishes a restored original regression and the stated focused
checks. It does not establish issue closure, draft promotion, whole-suite
compatibility or passing final-head CI.
