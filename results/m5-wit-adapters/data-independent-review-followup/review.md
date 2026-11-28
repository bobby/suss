# Independent follow-up review of #523 / #524

Disposition: the two original P2 findings are addressed; no new concrete review
finding remains for these repairs. Fresh full-workspace validation and passing
final-head CI remain separate promotion gates.

This review checks the two repairs requested in the independent review recorded
at `../review-524/review.md`. It does not replace the full workspace baseline
or the passing final-head CI required before draft promotion.

## Source qualification

Inspection began at HEAD `9bb6c3dcf8d704a141d571a8013ec4e40afd47ba` with
uncommitted production/test repairs. During the second independent execution,
the parent committed/published those bytes at
`04bfbb6a803e66378bc680af1005f20709bd0d66`. The receipt records the actual
start/end HEAD values. Qualification is by the unchanged individual source
and linked-library hashes in `receipt.json` and `repaired/receipt.json`, not
by claiming the second command ran with one unchanged Git HEAD.

The independent reviewer did not edit tracked files, Git state, or GitHub.
All harness sources, binaries, logs, and this report are ignored probe output.
No RUSTFLAGS override was set. Rust was stable 1.99.1. Executions used bounded
systemd scopes (CPU 100%, MemoryHigh 3G, MemoryMax 4G), the already-built libraries,
and did not start Cargo or acquire the recorder's Cargo lock.

## Code assessment

The schema-provider repair in `portable/aot/schemas.rs:7-19` validates every
selected function-free provider's declared data types before source macros.
The imported-function gate remains in `portable/aot.rs:375-403`.
`schemas.rs:22-80` encodes actual imported interface instances, versioned names,
implements/external IDs, exported nominal type identities, and aliases to those
actual types. This satisfies the minimal boundary direction from the original
finding; it does not simply remove the import rejection or replace names with
fresh structural copies. Cross-provider dependencies use existing outer type
aliases. The generic `TypeSink` in `portable/command/shape.rs` gives instance
types their correct type-index count, and `export_named` updates identities
introduced by type exports. Random function aliases and exported interface
instance indices include the schema-instance offset (`aot.rs:936,1253`).

The fixed-list repair retains a separate `Node.fixed_element` instead of treating
numeric fixed lists as indistinguishable tuples (`composite.rs:30-31,250-258`).
The projector accepts matching primitive arrays or numeric `vector-of` storage
only for numeric fixed lists, enforces the fixed length, and keeps ordinary
tuples on the native-vector path (`composite.rs:460-492`). Validation verifies
packed storage kind and length before canonical allocation (`composite.rs:1080`),
and writing uses the matching raw numeric storage (`composite.rs:1245`).
The u8 path preserves signed byte-array bit patterns, and numeric packed paths
avoid conversion through binary64. I found no new concrete defect in these fixes
on static inspection.

## Parent recorder independently inspected

I read the compressed raw command log and receipt under
`results/m5-wit-adapters/data-review-public-provider-and-packed/`.
The exact command was:

```sh
CARGO_TARGET_DIR="$PWD/target/probe/cargo" CARGO_BUILD_JOBS=1 RUSTUP_TOOLCHAIN=stable cargo test -p suss-compile --locked --test portable_aot_wit_review_regressions --test portable_aot_wit_value_shapes --test portable_aot_wit_composites --test portable_aot_wit_type_exports -- --test-threads=2
```

This command was executed by the parent, not rerun by this reviewer.
All 23 tests passed: six composites, six review regressions, two type-export
controls, and nine value-shape controls. Receipt: exit 0, 964.7604944808409 seconds,
`changed_inputs: []`, HEAD unchanged during that recorder, and 13,879 captured
inputs. The regression tests execute both original reproductions, all six
numeric packed kinds with integer extrema and float negative-zero bit checks,
three expected malformed result controls, an exported-interface instance case,
and schema plus random-provider indices. The existing controls execute nested
owned payloads, native collection functions, nominal aliases, malformed-value
preallocation behavior, GC retention, and 10,000 repeated nested transfers.

## Independent executions

The initial independent engine omitted `wasm_component_model_implements(true)`.
That run passed the original versioned provider and the reversed declaration /
provider dependency-order scenario, including an exact `9007199254740993` result,
then stopped at the repeated-provider component with Wasmtime's explicit
`the cm-implements feature is not active` validation error. This is a retained
harness configuration failure, not a production finding. Its exit 101, stdout,
stderr, immutable source/library hashes, and elapsed 254.644803 seconds remain
in the root receipt/logs. A preliminary Rust link also failed E0463 because it
selected a different Wasmtime feature artifact; both build logs are retained.
The repaired harness enables the required feature and links the exact Wasmtime
artifact used by the compiler, then independently executes the remaining cases.

## Independent case results and residual limits

Across the explicitly qualified commands, all four positive scenarios and all
three malformed-value controls execute as expected:

| Scenario | Executed result |
| --- | --- |
| Original nested/later imported type aliases | Versioned provider import preserved; native `assoc` yields outer value 255 after GC |
| Reversed declaration and provider dependency order | Resolver/encoder orders base before derived; exact s64 value 9007199254740993 survives native `assoc` and GC |
| Same provider selected as left, right, and package name | Three imports retained; empty linker instantiates; native `update` returns exact 9007199254740994 after GC |
| Nested fixed numeric lists | Unsigned byte-array returns 255,1,2; signed byte-array returns -1,-128,127; `vector-of :byte` returns unsigned 255,128,127 |
| Wrong fixed length | UncaughtException plus language message `WIT bytes requires its declared value shape` |
| Wrong numeric packed kind | UncaughtException plus language message `WIT fixed-length list returned a primitive array of another element type` |
| Packed array used for ordinary tuple | UncaughtException plus language message `WIT list requires a persistent vector` |

The final artifact-only negative inspector passed all three controls with exit 0
in 52.94225852796808 seconds. `negatives/receipt.json` records unchanged inputs
including the exact component SHA-256 and unchanged final HEAD 04bf. The complete
stdout/stderr are retained. Neither earlier exit-101 receipt is hidden or counted
as a passing whole command; those commands' completed positive assertions are
reported individually above.

The independent runtime exercises the new imported-name/dependency path and
additional fixed signed-byte/vector-of behavior. It is not an exhaustive proof
for every combination of schema aliases, imported interface metadata, packed
kind, async signature, or generic command encoder use. The parent recorder
adds executing coverage for exported interfaces, random-provider offsets,
all numeric packed storage kinds and negative-zero bits. The generic TypeSink
refactor also affects command assembly, so the fresh full workspace run remains
required. This follow-up did not rerun the historical five-hour baseline or
assert passing final-head CI.

Invalid float/nominal-enum map keys were already verified in the original review
to fail WIT resolution before deliberately throwing source macros; those remain
successful negative controls, not findings. The original own-handle/duplicate-map
hypothesis is separate #21 work: pinned Wasmtime rejects handle-containing map
values before calls, so the runtime leak hypothesis was ruled out. This follow-up
makes no resource-implementation or #21 acceptance claim.

## Exact reviewer build/run commands

All commands below ran from the data worktree:
`/home/bobby/.codex-workspaces/worktrees/ios-bc59f46b-2529-47d6-9d06-a888fafcdd03/suss/target/probe/m5-composites`.
The harnesses use the public `Compiler::compile` pipeline and Wasmtime execution,
not encoding-only checks. The `.rs` files retain the literal source/WIT/input
and assertion arguments. The Python runners retain the literal process argv
and collect input hashes, exit status, stdout, stderr, elapsed time and final HEAD.

First attempted link (retained harness dependency-feature mismatch, E0463):

```sh
env XDG_RUNTIME_DIR=/run/user/1000 DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/1000/bus systemd-run --user --scope -p MemoryHigh=3G -p MemoryMax=4G -p CPUQuota=100% rustup run stable rustc --edition=2024 target/probe/review-524-followup/probe.rs --extern suss_compile=target/probe/cargo/debug/deps/libsuss_compile-eacd3b495e585863.rlib --extern wasmtime=target/probe/cargo/debug/deps/libwasmtime-6231b9cb28d7d6e4.rlib -L dependency=target/probe/cargo/debug/deps -o target/probe/review-524-followup/probe > target/probe/review-524-followup/build.stdout 2> target/probe/review-524-followup/build.stderr
```

Matched compiler dependency artifacts (success):

```sh
env XDG_RUNTIME_DIR=/run/user/1000 DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/1000/bus systemd-run --user --scope -p MemoryHigh=3G -p MemoryMax=4G -p CPUQuota=100% rustup run stable rustc --edition=2024 target/probe/review-524-followup/probe.rs --extern suss_compile=target/probe/cargo/debug/deps/libsuss_compile-eacd3b495e585863.rlib --extern wasmtime=target/probe/cargo/debug/deps/libwasmtime-6dac808114cae8a2.rlib -L dependency=target/probe/cargo/debug/deps -o target/probe/review-524-followup/probe > target/probe/review-524-followup/build-matching-deps.stdout 2> target/probe/review-524-followup/build-matching-deps.stderr
```

Rebuilt after changing the unexecuted WIT fixture identifier `result` to `row`
(success; this edit affected only the ignored probe):

```sh
env XDG_RUNTIME_DIR=/run/user/1000 DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/1000/bus systemd-run --user --scope -p MemoryHigh=3G -p MemoryMax=4G -p CPUQuota=100% rustup run stable rustc --edition=2024 target/probe/review-524-followup/probe.rs --extern suss_compile=target/probe/cargo/debug/deps/libsuss_compile-eacd3b495e585863.rlib --extern wasmtime=target/probe/cargo/debug/deps/libwasmtime-6dac808114cae8a2.rlib -L dependency=target/probe/cargo/debug/deps -o target/probe/review-524-followup/probe > target/probe/review-524-followup/build-final.stdout 2> target/probe/review-524-followup/build-final.stderr
```

Initial execution (two data-provider cases pass, then missing engine feature):

```sh
env XDG_RUNTIME_DIR=/run/user/1000 DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/1000/bus systemd-run --user --scope -p MemoryHigh=3G -p MemoryMax=4G -p CPUQuota=100% python3 target/probe/review-524-followup/run.py > target/probe/review-524-followup/recorder.log 2>&1
```

Repaired harness build and execution (required engine feature added, already
passed first two cases omitted to avoid repeating compilation):

```sh
env XDG_RUNTIME_DIR=/run/user/1000 DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/1000/bus systemd-run --user --scope -p MemoryHigh=3G -p MemoryMax=4G -p CPUQuota=100% rustup run stable rustc --edition=2024 target/probe/review-524-followup/probe-repaired.rs --extern suss_compile=target/probe/cargo/debug/deps/libsuss_compile-eacd3b495e585863.rlib --extern wasmtime=target/probe/cargo/debug/deps/libwasmtime-6dac808114cae8a2.rlib -L dependency=target/probe/cargo/debug/deps -o target/probe/review-524-followup/probe-repaired > target/probe/review-524-followup/repaired/build.stdout 2> target/probe/review-524-followup/repaired/build.stderr
env XDG_RUNTIME_DIR=/run/user/1000 DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/1000/bus systemd-run --user --scope -p MemoryHigh=3G -p MemoryMax=4G -p CPUQuota=100% python3 target/probe/review-524-followup/run-repaired.py > target/probe/review-524-followup/repaired/recorder.log 2>&1
```

The separate Rust compiler invocations were awaited to completion before each
execution; they were not started concurrently. The first wrong-library build
has no runtime implication and is not counted as product-test failure. The
missing-engine-feature run is not counted as a successful repeated-provider
control. Its failure remains visible beside the repaired execution.

The repaired run then hit a separate diagnostic-harness assumption: wrong-kind
numeric list projection throws a structured language payload, while the harness
unconditionally decoded a raw UTF-16 array. Before that diagnostic panic,
repeated-provider and fixed-packed execution had passed, and both bad-length
and bad-kind had already satisfied the expected UncaughtException and pending
language exception assertions. This exit 101 is retained in `repaired/` with
238.58895020699129 seconds, no changed source/library/probe inputs, starting
HEAD 9bb6 and final HEAD 04bf. It is not called an all-passing execution.
A final independent inspector reuses exactly the emitted `fixed-negatives.wasm`
and decodes either raw strings or structured language payloads (as the existing
boundary tests do); it recompiles no Suss source and changes no production code.
Its exact commands are:

```sh
env XDG_RUNTIME_DIR=/run/user/1000 DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/1000/bus systemd-run --user --scope -p MemoryHigh=3G -p MemoryMax=4G -p CPUQuota=100% rustup run stable rustc --edition=2024 target/probe/review-524-followup/probe-negatives.rs --extern wasmtime=target/probe/cargo/debug/deps/libwasmtime-6dac808114cae8a2.rlib -L dependency=target/probe/cargo/debug/deps -o target/probe/review-524-followup/probe-negatives > target/probe/review-524-followup/negatives/build.stdout 2> target/probe/review-524-followup/negatives/build.stderr
env XDG_RUNTIME_DIR=/run/user/1000 DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/1000/bus systemd-run --user --scope -p MemoryHigh=3G -p MemoryMax=4G -p CPUQuota=100% python3 target/probe/review-524-followup/run-negatives.py > target/probe/review-524-followup/negatives/recorder.log 2>&1
```

## Fixed input hashes

These source/test/library hashes matched before and after both compiling/executing
review harnesses. The full receipts also retain harness hashes.

```text
0614eb2c8a2c6cdb1df5e008dc143c824700557c28f8644fdfd64556aad0d452  crates/suss-compile/src/portable/aot.rs
1b5917f2d625bc297bf6e7095e215fff3a8ba91afb10a68fcd7569bcc8cf72fa  crates/suss-compile/src/portable/aot/schemas.rs
b6f62784fa8b48422bde2242aa2bc3d3c3a2b5c772b3ff7a71b5162020ff681e  crates/suss-compile/src/portable/aot/composite.rs
807137cdb5963588c7ad70fc544a9d8dd86293b33cb462482d5b1b870840bbc8  crates/suss-compile/src/portable/command/shape.rs
75f5658518665dc28e08a01c5fb5ce1a5457c203559071a9badd269199221cd6  crates/suss-compile/tests/portable_aot_wit_review_regressions.rs
fdf134dd5e81c2e7d29066d5245468d0c384fdbab308fc8fdd8283cdd18e9035  lib:target/probe/cargo/debug/deps/libsuss_compile-eacd3b495e585863.rlib
0e8640d0a15ae764e83e95f6ad46cca8154b0deab3e4fd436cf74cf6e2571214  lib:target/probe/cargo/debug/deps/libwasmtime-6dac808114cae8a2.rlib
```

The final negative inspector additionally hashes the exact reused component
artifact. All produced `.wasm` artifacts are hashed here for reproducibility:

```text
1eefc58d055a3fd51a5b1862e1da732c8d1af129368871aa5b085c73a9805194  dependency-order.wasm
760c840b9ead28e81e475ef1814427113ccd27b5ecb192a485c46c4c18f88ad6  fixed-negatives.wasm
5377373dbbfd0fbb941b714fef51b04d9a15d2ed2c2eed6994c1eec158d181ce  fixed-packed.wasm
380bd4e1a6d4f7e1a7fa7a3a490b73ef0d87b4602d1eaaec621b19ac99223cd6  original-provider.wasm
4371c559292a64a9bedde29944d96d902841075bf5c777cfd0cf0a65714f5d3d  repeated-provider.wasm
```
