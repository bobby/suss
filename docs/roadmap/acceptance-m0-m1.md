# M0/M1 acceptance reconciliation

Audit date: 2026-09-29. Implementation was merged by the user in
[PR #40](https://github.com/bobby/suss/pull/40); this reconciles tracking, not
new compiler capability. The accepted criteria in `issues.json` and the
published issues are unchanged. Every criterion below has executing or
source-policy evidence. Earlier handoff notes saying acceptance awaits PR #40
review are historical and superseded by its completed independent review,
pushed significant fixes, successful final-head CI and user merge.

Reviewed implementation: `fb3ec0a2e438d0f295795ab843992735e023ba9c`;
[CI 36636314840](https://github.com/bobby/suss/actions/runs/36636314840) passed.
Merged main: `fad9ee929224cec46265fb92b1acd3c9db82bab1`;
[CI 36639533936](https://github.com/bobby/suss/actions/runs/36639533936) passed.
Fresh checks in the reconciliation worktree use that main commit.

| Issue | Acceptance evidence | Scope and remaining work |
| --- | --- | --- |
| [#1 M0-01](https://github.com/bobby/suss/issues/1) | Two independent `cljs_inventory.generate()` calls match each other and tracked inventory exactly: 1,065 declarations, SHA256 `b6f3bce5e2847efd0eabee1b61c83914be1c659f41c6f51b04fdc4bcb8509f8c`. Records retain source ranges/hashes and reader branches. `cljs_reviews.py` verifies the pinned overlay schema; scanner/review negative regressions and `docs/compatibility/PROVENANCE.md` establish review/source-license policy. | This completes the inventory/schema/policy requirement. All 1,065 declarations are unassessed; review, extraction and core implementation remain M4/M7. |
| [#2 M0-02](https://github.com/bobby/suss/issues/2) | Eight `toolchain_profile` and eight `toolchain_async` executing tests pass: GC/function references/tail calls/EH; canonical maps; implements/external-id; async callbacks; futures and streams with payload reads, bounded demand/EOF and cancellation. Exact Wasmtime 49.0.1, wasm-tools family 0.258.0 and wit-bindgen 0.61.1 are locked; `wasi_lock.py` verifies 15 files/six official packages. Prototype async/boundary-shape diagnostic tests explicitly reject unsupported production paths. | Feasibility is proved. Production compiler adapters, canonical ownership/memory, full WASI packages and async scheduling remain M5/M6. Optional compiler-component build failure remains recorded in handoff. |
| [#3 M0-03](https://github.com/bobby/suss/issues/3) | Three `shared_runtime` tests execute independently compiled fragments in one Store. Identical recursive types share closures, nominal descriptors and runtime roots. Forced GC preserves captures after rebinding. Declared ABI mismatch and actual layout mismatch reject before initializer effects. The version-gate negative regression failed when bypassed. | Probe loader is not a production persistent REPL. Binding/session lifecycle and unified compiler fragments remain M2/M3. |
| [#4 M0-04](https://github.com/bobby/suss/issues/4) | Fresh Chrome 154.0.8037.58 run passes exact required feature checks, ES module loading, typed result, WasmGC continuation retained across Promise suspension/resume, cancellation, stale callback isolation and feature compile rejection. Optional Jco 1.35.0 produces executable ESM/core GC and typed u32 result 42. `docs/roadmap/toolchain.md` documents both routes; five validator regressions reject absent/partial results. All 11 harness/source hashes and three generated artifact hashes match committed `browser-probe.json`. | Single-browser feasibility fixture. Chrome teardown times out after completed DOM: `browser_process_exit_code` remains null and termination flag true. Clean process shutdown, cross-browser/product output and canonical typed host corpus are not claimed; product delivery remains M8. |
| [#5 M1-01](https://github.com/bobby/suss/issues/5) | Nine `conformance` tests pass, including independent decoding of nested scalar/collection/trie values; wrong collections/scalars; executed unknown/malformed layouts; malformed/missing input; duplicate evidence; exact failure/unexpected-pass comparison; and traps distinct from language exceptions. All 201 reviewed legacy cases pass with zero known failures/skips. Two manual catalog/capture tests remain explicitly ignored. | A strict harness and curated baseline are complete; this does not establish full portable core compatibility or decode arbitrary future layouts. |
| [#6 M1-02](https://github.com/bobby/suss/issues/6) | Fresh `scripts/test-oracle.sh` compiles the pinned ClojureScript reference, executes Node and Suss Wasm, independently decodes GC results and compares lossless tags. Its 16 cases include condition-once, ordered arguments, binary64 bits/signed zero/NaN/Inf, lone/pair surrogates, reduce/variadic arities and thrown values/ExceptionInfo/cleanup effects. Four Rust oracle tests pass; one manual capture is ignored. Python transport/comparison negative regressions reject missing/changed values, effects and failure stages. JVM/Node occur only in development oracle tooling, not shipped code. | Evidence-harness acceptance is complete: **9 differential passes, 7 exact failures, 0 skips**. Known failures remain failures. Numeric/UTF-16, sequence/rest and ExceptionInfo repairs plus comprehensive arity/semantic coverage remain M2/M4/M7, not an unfinished oracle transport contract. |
| [#7 M1-03](https://github.com/bobby/suss/issues/7) | CI runs locked Cargo dependencies/submodules, inventory/hash checks and Python tests with two build jobs, two test threads, a 25-minute job timeout and cached dependencies. Shared test engines and bounded fuel/decoder traversal are implemented. Focused and full baselines pass locally; reviewed-head and merged-main Linux CI pass. Manual ignores and exact failures are reported separately. | Bounded reproducible prototype evidence is complete. Future runtime/compiler changes must continue to pass required checks; release delivery remains M2–M9. |

## Fresh commands and evidence

- Two independent `cljs_inventory.generate()` calls; `python3 scripts/cljs_reviews.py`; `python3 scripts/wasi_lock.py`: passed, exact hashes and unassessed counts retained.
- `python3 -m unittest discover -s scripts -p 'test_*.py'`: 42 passed.
- `CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target cargo test -p suss-compile --test toolchain_profile --test toolchain_async --test shared_runtime --test conformance --locked -- --test-threads=2`: passed; 8 + 8 + 3 + 9, two manual conformance ignores. Log `/private/tmp/suss-acceptance-focused.log`.
- `CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target scripts/test-oracle.sh`: passed, fresh reference observations and unchanged 9/7/0 differential baseline. Log `/private/tmp/suss-acceptance-oracle.log`.
- `python3 scripts/probe_browser.py --chrome '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome' --wasm-tools /private/tmp/suss-toolchain-49/wasm-tools-1.258.0-aarch64-macos/wasm-tools --jco /private/tmp/suss-jco-1.35.0/node_modules/.bin/jco --output /private/tmp/suss-acceptance-browser.json`: semantic/packaging fixture passed, recorded teardown limitation retained. Fresh JSON matches committed evidence exactly.
- `CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target cargo test --workspace --locked -- --test-threads=2`: full baseline passed, exit 0; log `/private/tmp/suss-acceptance-full.log`. Existing manual/legacy ignores remain unchanged.
- `python3 scripts/publish_roadmap.py`: offline preview passes, 10 milestones/39 stable issues. It does not close or overwrite remote issues.

The reconciliation PR uses explicit `Closes` links for #1–#7. Their remote state
remains open until that PR merges; no agent merges PRs. M0/M1 milestones remain
open until all linked issues close and their exit gates are rechecked. #10 from
PR #41 and #8 from PR #42 remain partial; use `Refs` links for those increments.
No M2–M9 issue or milestone is completed by this audit.

Next unblocked implementation: lossless reader forms -> HIR with binding
identity/source spans -> explicit evaluation-order/control-flow IR -> shared
runtime lowering. Preserve once-only source order and do not convert through
legacy EDN. Retire prototype paths only after replacement acceptance passes.
