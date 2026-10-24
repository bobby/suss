# Retaining normal CI bootstrap reproductions

The bootstrap verifier normally generates two private output directories, compares
both phases byte-for-byte against each other and the shipped files, checks identity,
and executes compiled_bootstrap. Its cleanup discarded generated files on a shipped
mismatch, preventing inspection/reuse of the actual normal generation while the
separate local baseline occupied the sole local Cargo producer.

An optional SUSS_BOOTSTRAP_DIAGNOSTICS_DIR now retains both complete generations
before the unchanged comparisons. The directory must be fresh. Eight files, their
byte lengths/SHA-256 digests, actual checkout head/tree/clean status and separate
reproduction/shipped-match booleans are recorded; receipt.json is published last.
A missing generated file or existing destination is an error, with no deletion of
prior evidence. The receipt is byte/source evidence, not artifact validation,
execution or acceptance. Reuse still requires source/identity verification and all
normal validation/execution gates.

CI supplies a runner-temporary diagnostic directory and uploads it using the
[current official upload-artifact interface](https://github.com/actions/upload-artifact).
The diagnostic step runs after failures except cancellation; absent outputs are
optional, while the original failed generation/comparison/identity/test step still
fails foundation and the aggregate required check. Triggers, concurrency, eight
integration shards, timeouts, no-fail-fast policy and existing tests are unchanged.
This retains CI diagnostics; it does not implement compiler caching/build-directory
or partial-compilation reuse from issue272.

Four focused Python tests pass, including a synthetic-generator shell test proving
both nondeterminism and shipped mismatch still return failure while retaining files.
Other cases check complete hashes/source receipts, dirty source, incomplete outputs
and preservation of prior evidence. Shell syntax and whitespace checks pass. The
synthetic generator executes no real Cargo/compiler/Wasm code; these results confer
no native bootstrap, macro or conformance credit. Independent source review verified the four exact postimages, retained failure
gates, source/hash receipt and documented action interface, returning GO SOURCE-ONLY.
Actual CI upload/generation remains required before relying on the new receipt path.
