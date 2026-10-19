# Separate REPL compilation fuel configuration

Published source: `49d7ea349dfb051cb1916b12ff07a7be21749ff3`. The three committed Rust files match the final V4 executed postimages exactly. Source planning began at published19def; both controlled executions used the later docs-only basecd65a2ee3c4244ac11f4b5ab287b5ef249375c89. Exact controller manifests, original source bytes, overlays, input freezes and restore receipts are retained.

`--fuel N` preserves legacy behavior: it configures evaluation and compilation fuel. `--compile-fuel N` overrides compilation and macro-expansion fuel independently of flag order. Both options default toNone and leave the existing10M defaults unchanged. One private startup helper is used by Unix and non-Unix frontends; no new public getter or Session/reset implementation change is introduced.

Input preparation consumes compilation fuel even when no macro is invoked. Existing accounted reader/form/HIR/IR/expansion work and Macro-store operations use this policy; this is not a claim to meter all compiler work. Engine JIT work and process memory are outside this fuel accounting. Increasing an explicit allowance permits more work; it does not accelerate compilation and supplies no acceptance credit under original no-increase criteria.

## Actual execution scopes

V3 normal bootstrap and identity PASS. Its14 test executions report13PASS/1FAIL/0ignored: five parser tests, two real option/reset tests, six unchanged legacy preservation tests pass; the first new subprocess fixture fails its own strict isolation criterion. Its256-iteration macro returned42 but the measured expansion cost was36,346, below Runtime50,000. This is a deficient new cost fixture, not an observed production failure. The first flag order reached that criterion; the second order and Runtime loop/recovery tail were unexecuted within that failed test. Raw failure and selected tails are retained.

V4 changes exactly that new fixture's iteration literal256→1024, with identical production source, budgets, assertions, timeouts and all other tests. V4 normal bootstrap/identity PASS; only the corrected new subprocess test runs and passes1/0/0 in98.19s. Both flag orders report actual Macro cost134,650 with compilation fuel10M versus Runtime50k. Each real CLI child returns42, then reports typed Runtime exhaustion for an infinite ordinary loop and evaluates43 afterward. Actual underlying option APIs separately verify that evaluation fuel was not raised by the compilation flag.

These scopes establish14 distinct positive test outcomes across production-identical V3/V4 tuples, with the initial fixture failure still retained:15 raw executions,14PASS/1FAIL. This is **not a fresh14-test run on final V4 test source**, not14 new tests, and not a passing full baseline. V1's incorrect macro-only scope and V2's pre-help designs were prepared but UNEXECUTED; no runtime credit is assigned to them.

The five parser controls cover defaults/aliases, flag order, independent repeated flags, errors and unsigned zero/MAX behavior. The two real API tests call the exact private startup helper and inspect existing Session.options().fuel_per_operation and public ExpansionHost::expansion_work_fuel. Noflag, legacy-only, compile-only, combined and zero options are checked. A separate finite Runtime10M/Compile20M reset succeeds, preserves both values before/after reset_compiled and evaluates43. Low Runtime50k process reset is deliberately not required: core reinstallation must also fit configured budgets. No setup-budget increase or hidden reset-policy repair is made.

Six existing frontend regressions pass for async fuel recovery, reset, retained captures/effects, compiled macro effects/old expansions, namespace/reset and failed-reset preservation of both phase stores. All original test bodies, budgets and deadlines remain unchanged. The new process fixture does not imply global AOT/configuration coverage.

Both controllers are terminal/drained; all executed source freezes match each stage, and full saved freezes restore to their pre-overlay states. All four bootstrap payloads are FULL-byte identical across V3/V4 and their original published base. They are Git dependencies, not re-created or duplicated in this archive. Normal bootstrap/identity verification succeeded in both runs; no semantic execution is invented from an encoding check.

## Source and remaining limits

The archive is content-addressed cache evidence with an explicit source commit and unchanged bootstrap Git references, not a standalone whole repository. It includes actual3Rust postimages, source variants/preimages/patches, commands/environments, raw logs, stage/terminal receipts, input hashes, original failure, correction and test documentation. No build targets are included. Manifest hashes resolve every included logical file to a safe regular blob.

Focused configuration results do not waive existing original inference/CoreEnv/collection default-fuel failures, establish issue252 completion, or change a default. Final-head CI and required baseline remain separate review gates. No issue closure, promotion or merge is claimed. Command and environment details are in each run.py/receipt; no RUSTFLAGS were set.

Archive SHA256: `3490a570cae944f5b8448c51dfca167c6e4103733cf9fcb62d7ca19860c0c253`. Exact archive link is supplied by main when copying into repository evidence.

[Raw configuration evidence](m4-pr252-repl-compile-fuel-2026-10-18.tar.gz) — SHA256 `3490a570cae944f5b8448c51dfca167c6e4103733cf9fcb62d7ca19860c0c253`. The archive contains65 safe regular members,64 content-addressed payloads and135 logical bindings.
