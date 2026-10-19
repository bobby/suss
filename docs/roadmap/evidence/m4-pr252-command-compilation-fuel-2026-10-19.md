# Public command compilation allowance and retained factory regression

Base: `8921f864555a60df69c8f2bd6d8abe73d4064810` on PR #256. This follow-up
adds `Compiler::compile_for_main_with_compile_fuel(source, namespace, allowance)`
by exposing the existing internal command preparation option. The legacy public
method delegates `None`. Explicit `Some(0)` is not treated as default. Namespace
validation still precedes Macro preparation; Runtime initialization remains
deferred to the artifact host. The Wasm-host unsupported branch is preserved.
The command pipeline, profile, dependencies and exit policy are unchanged.

## Actual failures and bounded repairs

Base c0c9c45 CI integration6 completed92passes/1failure across21of32planned
targets, leaving11unexecuted. The failure was the original public command
lexical-macro fixture, caller &env transport OutOfFuel179..471. The shared
semantic10B allowance is supplied only to that fixture's compilation; its entire
source, ordered empty/Unicode/flag arguments, positive/negative runs, profile
checks and assertions are preserved. All other original fixtures still call the
legacy default API. A new executing control checks namespace validation before
throwing macros under None/zero, explicit-zero semantic fuel failure, recovery on
the same Compiler through None/10B, and legacy default recovery. All successful
command variants are validated as official async WASI components and executed
with the expected explicit exit73. Existing initializer/capability tests remain.

Base c0c9c45 CI integration7 completed29passes/1failure across8of32planned
targets, leaving24unexecuted. The failure was the original map-factory fixture,
caller &env transport OutOfFuel137..171(inner4756..4771). Only that fixture's
CompiledMacros allowance becomes10B. Its complete original file is recovered by
removing the import and one setter. Caller Runtime, both phases, missing/nil/
false distinctions, extension entries, factories, GC and decoder checks remain.
The other original authored factory case and three adversarial decoder tests
retain their limits.

CI Cargo shard commands now include `--no-fail-fast`: Cargo continues to later
selected harnesses after test failures and still returns nonzero if any fails;
compilation errors or timeouts can still prevent completion. Python's check=True
continues to propagate that failure. This exposes later failures instead of
leaving most targets unexecuted behind the first red harness. Partitions, test
selection, eight shards, maxparallel4, integration120-minute window, Runtime,
default/exhaustion controls and testthreads2 are unchanged. All four planner
unit tests pass on the candidate and integrated source.

## Executed validation

The preceding complete upstream harness finishes12PASS/0FAIL/0ignored in621.76s
(622.21s including build) on the8921f86 native/test source, before this follow-up.
This includes the exact reviewed corpus baseline, original200M map control,
reader15/when-let12 ordered pinned observations and adversarial decoding. It is
qualified previous-source evidence, not a fresh final-native workspace verdict.

With CARGO_BUILD_JOBS=1/RAYON_NUM_THREADS=1 and the existing external target dir:

```sh
cargo run --profile test -p suss-cli --bin suss-bootstrap --locked -- runtime/bootstrap
python3 scripts/verify_bootstrap_identity.py
cargo test --no-fail-fast --locked -p suss-cli --test public_compiled_command --test compiled_record_factories -- --test-threads=2
python3 -B -m unittest discover -s scripts -p test_ci_test_shards.py
```

Fresh normal bootstrap generation passes20.55s; identity verification passes
0.91s. Both phases' six non-custom Wasm sections match the previous images;
compiler source identity/custom metadata and image hashes are refreshed. No
bootstrap instructions change. Independent source review approves the public
API, exact original-fixture preservation and CI failure propagation.

Actual complete target results: **5PASS/0FAIL/0ignored** factory tests in
337.40s, and **6PASS/0FAIL/0ignored** public command tests in635.89s.
The combined Cargo invocation terminates0 in994.96s including compilation.
Both original CI failures now execute successfully; the new public zero/default/
preflight/recovery control also passes. No other test is filtered or ignored.


These focused results do not replace the exact unfiltered required workspace
command, Java/Node-free byte-exact reproduction or final committed-head CI.
No issue closure, draft promotion, merge or full-core conformance is claimed.

[Retained evidence archive](m4-pr252-command-compilation-fuel-2026-10-19.tar.gz), SHA-256 `9c8fd6a56dea8ee2c2cbf17d500cfdeca8b718e3c4ed458b69626f758ce89883`. Its manifest binds all payloads.
