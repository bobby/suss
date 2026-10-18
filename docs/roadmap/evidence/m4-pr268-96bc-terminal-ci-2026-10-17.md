# Terminal CI evidence for PR #268 head 96bc8d7

Tested commit: `96bc8d70ae447d0dba1e89a3c03e97ce0a0f84bb`. GitHub Actions run [39093215076](https://github.com/bobby/suss/actions/runs/39093215076), event `push`, attempt 1, completed with **failure**. Foundation succeeded; all eight integration shards failed; the aggregate test job failed.

Foundation executed **536 passing Rust tests, 0 failures, 1 ignored**, and **353 passing Python tests**. The bootstrap verifier succeeded, including its two reproductions/four artifact comparisons and identity checks; four compiled-bootstrap tests passed.

Integration shards executed **111 passing tests, 14 failing tests, 0 ignored**. All fourteen failures are guest-fuel failures: eleven caller-&env input-transport failures and three compiled macro-invocation failures. Exact target/name/diagnostic rows are retained in `carver-independent-terminal-review.json`. All fourteen failed names and failure stages match the audited parent `ae28db396ee3050dedca812ff6993d7998e3e27f`; this is a comparison of observed failures, not proof that later targets pass.

The source-derived integration plan contains **259 targets**. **30 targets started and completed; 229 targets were unexecuted** because of fail-fast behavior. There are no started-but-unfinished targets in these terminal logs. Counts describe executions in this CI run, not a union with local focused tests.

The bundle retains the complete workflow ZIP (89 members), all ten root job logs, terminal API metadata, earlier pending-state metadata, eight early terminal shard log captures, the host audit script and review, and the parent comparison review. Nine actual checkout SHA proofs match the tested head. They are checkout/tree proofs, not nine custom source-hash or clean-worktree proofs. Committed compiler/runtime/scripts/workflow trees match source parent `290e20eb8fa925d3e00faf99def0d4b7388aa684`; the four committed bootstrap SHA-256 values and workflow/verifier/shard-script Git dependencies are recorded separately.

This is a Git-resolved evidence bundle, not a standalone source archive. The audit script uses the public Git objects and included parent review; its original absolute cache layout is retained in member paths. No CI rerun, source edit, Cargo execution, quota change, or passing-CI claim was made by the reviewer. This evidence does not include or credit the separate native-prefix 27-test result, its initial module-path compile failure, or the pending field-plan artifact codec. It does not establish issue acceptance, readiness, merge approval, or closure.
