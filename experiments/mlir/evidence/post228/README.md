# Post-#228 execution and publication audit

These receipts preserve the completed runs at PR #227 head
`b82ad94cf7545a4f80315af3a6f677e57fc4bd13`, subsequently merged as
`f4b13f306ad354c6cbe5bc0c49cc44975249396d`. Before this documentation update,
all 184 experiment files, 120 production files and four bootstrap artifacts
matched the executed identity manifest exactly. This content comparison does not
claim a new execution at the later documentation head or at future compiler heads.

[Audit](audit.json) maps all seven #188 criteria to bounded evidence and limits.
The nine execution gates completed successfully. The exact-head full baseline
passed 1,550 tests, with zero failures and 41 existing ignored tests in 207 result
blocks. The full baseline log and its verified SHA256 are retained. Ten final-head
CI checks succeeded in [run 37875362660](https://github.com/bobby/suss/actions/runs/37875362660).
Root CI does not execute isolated LLVM/C++ or bridge gates; their separate logs
and executed identity are retained.

The raw CI capture was taken while the local baseline was still live. Its pending
`local_baseline` field is preserved as historical data and is superseded by the
terminal baseline receipt and full log; it is not evidence of another run.

Commands preserve the actual external tool paths used, and reproducible setup is
documented in [reproduce.md](../../reproduce.md). Old post-#226 and before-main
evidence remains unchanged. Binary hashes identify the executed snapshot; later
reuse of build directories does not imply those binaries still have the same hash.

Recommend **deferring adoption**: no maintenance saving is established, and the
slice still needs local semantic verifiers and the existing WasmGC backend.
No LLVM binary is shipped, no general source-to-MLIR compiler or scheduler is
claimed, and adoption requires a separate reviewed architectural decision.
This documentation checkpoint has its own review/baseline/CI gates before promotion.
