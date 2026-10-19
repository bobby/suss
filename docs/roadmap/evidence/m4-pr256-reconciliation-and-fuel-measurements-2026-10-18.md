# Reconciliation, compact canonical heads, and fuel diagnostics

These are separate source-bound executions, not a combined acceptance total.
Production compiler/bootstrap source is `d38b59cb1cbaff0def4aad257c79e4f28ebe339a`.
Main reconciliation was executed at `440a0b7735a4c290eb7c291cfe29e239b5ac33d9`.

| Execution | Observed outcome | Limits |
| --- | --- | --- |
| Main reconciliation | 7 Rust passes / 2 source-tag fuel failures; 5 Python passes | when-let target 6 passes and pinned wrapper 1 pass; all 12 pinned IDs host-pass. Earlier failed attempts and unexecuted tails remain archived. |
| Compact canonical heads | 43 passes / 4 fuel failures / 0 ignored | Raw keys 22, guards 20, pinned map 1 pass. CoreEnv, collection and both source-tag cases fuel-fail. |
| Exact-head CI 39149952174 | Foundation: 398 Rust passes, 1 existing ignore; 353 Python passes. Integration: 124 passes / 18 fuel failures | 31/251 integration targets completed; 220 unexecuted. Nine synthetic checkout trees match d38 exactly. |
| Workspace baseline | Exit 101: 86 passes / 2 fuel failures / 0 ignored across 17 harnesses | 14/251 integration targets completed; 237 unexecuted. Three unit harnesses completed; doctests not reached. |
| CoreEnv diagnostic, Macro allowance 200M | Exit 101: 0 passes / 1 fuel failure | Caller default and all original assertions unchanged; first caller &env transport still fails at 0..33. No semantic row or completed expansion interval. |

## Commands and source correspondence

The workspace baseline ran exactly:

```sh
cargo test --workspace --locked -- --test-threads=2
```

It took 3299.75 seconds. Both failures are in
`compiled_macro_collection_source_asts`: compiled macro invocation at 709..738,
and unordered literal caller &env transport at 510..531. Every one of its
3526 frozen input hashes matches d38 Git blobs; the terminal receipt reports
`changed: []`. The later observer and diagnostic test overlays are separate
executions and must not be interpreted as baseline inputs.

Focused argv, source/BOOT tuples, raw logs, receipts, parsers and independent
reviews are retained in the respective archives. The compact-head run also
binds all 3526 frozen files to d38. Its pinned map retains all 69 ordered IDs:
62 host-decided passes and seven guest judgments, with no failed, skipped or
unexecuted assertions. Its existing 200M harness allowance is unchanged; these
69 observations are not 69 independent Rust tests or default-budget evidence.

Compact canonical heads reduce native hash slots from 24 to 16 bytes on this
host, retaining full hashes, exact collision equality and checked stable IDs.
The paired CoreEnv run completes 182 versus 172 declarations, with 223016 more
fuel at the last common checkpoint. Collection has 83022 more fuel at its last
common checkpoint. Both still fail; incomplete required totals remain unknown.
These matched-prefix gains do not establish wall-time speedups or global
absence of regressions.

## Larger-budget diagnostic

The diagnostic changes only the test's Macro-session allowance after creation:

```rust
macros.set_operation_fuel(200_000_000);
```

It executes the original CoreEnv test, including both caller phases if reached,
with `SUSS_MACRO_FUEL_TRACE=1` and the original caller default. Actual first
request balances are start 199999819, provenance 199996363, and caller-form
199996362. It exhausts during caller &env construction before the next marker;
the second invocation and later caller phase are not reached. The full cost is
unknown, rather than estimated from an unfinished interval. Raising the Macro
allowance by 20x is insufficient for this test at this source.

All diagnostic inputs stayed unchanged and the known temporary test bytes were
restored to production. The diagnostic does not revise defaults, rates or issue
acceptance. The parser/controller have independent reviews; 12 synthetic parser
checks cover interleaved/prefixed output, zero tests, compile failure, incomplete
or nonmonotone chains, changed inputs and failed restoration. Synthetic outcomes
are separate from guest execution results.

## Reconciliation qualifications

The main adaptation preserves the original oracle-captured fixture and an exact
patch for the stage-aware language-exception diagnostic. Historical oracle
inputs, effects and expected values are not rewritten. Five provenance checks
include one positive and four negatives. Three added provenance inputs were
first copied after the driver began and separately hash-verified; they are not
claimed as pre-run frozen inputs. The original failures remain archived.

## Archives

- [m4-pr256-main-reconciliation-2026-10-18.tar.gz](m4-pr256-main-reconciliation-2026-10-18.tar.gz) — SHA256 `d1e7d73445a8f28a7b71b7cc88c28a59ec3308bdd38e3ae0ab71707c63cab7f3`
- [m4-pr256-compact-canonical-heads-2026-10-18.tar.gz](m4-pr256-compact-canonical-heads-2026-10-18.tar.gz) — SHA256 `616a17c5bade300f1e5b16e22e02afdd63694e09616f1602e0f7ab685461e0b8`
- [m4-pr256-d38-terminal-ci-2026-10-18.tar.gz](m4-pr256-d38-terminal-ci-2026-10-18.tar.gz) — SHA256 `f06ca92173b96758e4e04e384aedc9cd7f1ceec0f80b381d8cf3a760f68b474f`
- [m4-pr256-d38-workspace-baseline-2026-10-18.tar.gz](m4-pr256-d38-workspace-baseline-2026-10-18.tar.gz) — SHA256 `3342cfa792210eb383289f44a7d173b70d10ae1695aedab99d9291f985ffe8ea`
- [m4-pr256-coreenv-200m-diagnostic-2026-10-18.tar.gz](m4-pr256-coreenv-200m-diagnostic-2026-10-18.tar.gz) — SHA256 `2adfe9c8e2e10337b144a72300b3f4694fa37c027ac225dc73dd84f2baef40b9`
