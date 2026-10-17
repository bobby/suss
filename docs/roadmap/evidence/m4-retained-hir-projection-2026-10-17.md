# Retained HIR projection: commands, results and limitations

Candidate source: `5e14a6b8541921aca694ea41574fb69bfbb7624a`, derived from `3a3898c`. This evidence
separates original owned-HIR cloning from retained-root projection work. Source
AST and lowering jobs retain genuine immutable HIR owners or checked slice
positions. Private two-pass BFS projection uses exact node backing and direct
child ranges. All exported lowering fields and original source publication
checks remain; no content interning, guest hash substitution or fuel increase.

`python3 run-hir-projection-focused.py` first failed to compile V4: an owned
`FieldBinding.access` was passed as Arc, and the diagnostic helper needed a
static message lifetime. No tests executed in that attempt. V5 restores the
metered owned fallback and corrects that lifetime, without changing assertions.
Both source packets, failure log, receipts and input maps are retained.

The V5 retry regenerates the normal bootstrap and passes 26 focused executions:
projection 3, async-payload/ownership 3, sharing 20. Default CoreEnv and the three
original record/reduction witnesses still exhaust their unchanged allowances
before criterion observations: 26 pass/4 fuel failures, zero ignored. These are
four failed tests, not successful semantic observations. The duplicate default
CoreEnv diagnostic receives no additional unique coverage credit.

Measured fixture evidence: the four-node metadata-heavy example charges 457
for projection versus 5,232 for an owned HIR copy. Retained/fallback queue costs
are 348/1,007 without capture and 436/1,095 with capture. Depth/width 16 and 64
charge 2,156/8,252. Those are actual bounded native fixtures, not a whole-CoreEnv
saving estimate. Borrowed fallback still performs its HIR clone plus projection;
projection/scalar UTF16 payload copies remain performed and paid.

`python3 run-hir-projection-preservation.py` supplies terminal source/domain/raw-key,
CORE17, full map, pinned bitwise and analysis-graph outcomes:
89 passing executions and one diagnostic repeat fuel failure (no additional unique test credit), zero ignored. The combined focused/preservation result is
119 unique selected tests: 115 pass, 4 fuel failures, zero ignored. Raw per-attempt execution counts remain separate in
TERMINAL-RESULTS.json; the failed diagnostic repeat is not a fifth unique failure.

The complete unchanged source-child fixture has 220 ordered labels/results equal
to parent 3a. Both requests start with 10,000,000 and finish with 129,753; actual
net difference is zero on this fixture. Full phase comparison:
`{"Runtime": {"candidate_start": 10000000, "candidate_final_stage": "final-validation", "candidate_final_result": "Ok(())", "candidate_remaining": 129753, "control_remaining": 129753, "net_removed_at_final_marker": 0}, "Macro": {"candidate_start": 10000000, "candidate_final_stage": "final-validation", "candidate_final_result": "Ok(())", "candidate_remaining": 129753, "control_remaining": 129753, "net_removed_at_final_marker": 0}}`. No sparse/AST-path coverage or broader gain is inferred.

The default diagnostic fails in namespace declaration preparation at `-pop!`,
after declaration-origin with 7,273 remaining; queued Callable/AST processing is
not reached. Its 128 completed metadata-region intervals total 714,710, including
normalization and recipe work, not isolated copying. Historical 212M pre-root and
6.6M declaration-tail figures are not attributed to this optimization.

Parent-only context: exact 3a CI run 39060778549 is terminal failure. Foundation
has 524 Rust passes/0 failures/1 ignored and 351 Python passes; integration has
111 passes and the same 14 source71 fuel-failing names. Thirty of 255 integration
targets execute; 225 remain unexecuted. Raw ten job logs, nine checkout proofs and
independent classification are retained. This is not candidate final-head CI.
The separate 3a full workspace baseline is live at drafting time; its partial
log is excluded and no terminal/baseline success is claimed. Candidate full
workspace and final-head CI remain separate required evidence.

This archive is a THIN SUPPLEMENT, not standalone. It requires both exact files:

- `m4-boxed-integer-bits-checkpoint-2026-10-17.tar.gz`, SHA256
  `266d1ce35b4014cbe9eb67fdcf20220cd2dcec0784f0aecb34ef7412e7cdc88c`.
- `m4-invoke-unsigned-upper-bound-2026-10-17.tar.gz`, SHA256
  `6455ee46741981db166cfc593711236681ea584008c39ed6e202803dd9f5a9f5`.

BASE-AND-SOURCE-RESOLUTION.json maps every hash in every 2,988-input freeze to
verified dependency bytes or new supplement blobs. Final candidate committed
Git/pinned-CLJS binding is required. No milestone completion, acceptance, issue
closure or merge result is asserted.

Evidence: [thin supplement](m4-retained-hir-projection-2026-10-17.tar.gz), SHA256 `3971b9301e31abcf2fa16f3720ec1c9a491e0f82e11063ac546fd67fca120f90`; 130 members/129 manifested payloads. Both dependency archives listed above are required.
