# Reproduce the bounded evaluation

Run from the repository root. The tested platform is macOS ARM64 with Command
Line Tools, CMake, Python 3, Rust/Cargo, Node v24.5.0 and Clojure CLI available.
Java/Node are development oracles only. Linux distribution is not demonstrated.
Do not overlap Cargo graphs or reuse another worktree's target for path crates.
No RUSTFLAGS override is needed. Run C++ builds with one worker.

## Acquire the pinned SDK

Choose a disposable directory for `SUSS_MLIR_TOOLS`. The release URL, archive
size/hash, source commit and license are recorded in `toolchain-lock.json`.
The following downloads and verifies before extraction; pipefail is required.

```bash
set -euo pipefail
SUSS_MLIR_TOOLS=/private/tmp/suss-mlir-reproduce
mkdir -p "$SUSS_MLIR_TOOLS"
python3 - "$SUSS_MLIR_TOOLS" <<'PY'
import hashlib, json, pathlib, sys, urllib.request
lock = json.loads(pathlib.Path('experiments/mlir/toolchain-lock.json').read_text())
p = pathlib.Path(sys.argv[1]) / 'LLVM-23.1.3-macOS-ARM64.tar.zst'
with urllib.request.urlopen(lock['archive_url']) as source, p.open('wb') as output:
    while chunk := source.read(1 << 20):
        output.write(chunk)
assert p.stat().st_size == lock['archive_bytes']
digest = hashlib.sha256()
with p.open('rb') as source:
    while chunk := source.read(1 << 20):
        digest.update(chunk)
assert digest.hexdigest() == lock['archive_sha256']
PY
zstd --memory=1024MB -dc "$SUSS_MLIR_TOOLS/LLVM-23.1.3-macOS-ARM64.tar.zst" | tar -xf - -C "$SUSS_MLIR_TOOLS"
SUSS_MLIR_SDK="$SUSS_MLIR_TOOLS/LLVM-23.1.3-macOS-ARM64"
"$SUSS_MLIR_SDK/bin/mlir-opt" --version
"$SUSS_MLIR_SDK/bin/clang++" --version
```

Retain the verified hash when removing the compressed download. The extracted
SDK is about 7.6 GiB; the archive requires a 1 GiB decompression window. LLVM uses
Apache-2.0 WITH LLVM-exception. No SDK/exporter binary is shipped in this PR.

## Configure and build C++ tools

These are the executable configure flags used by the successful pinned build.
The empty build type is deliberate and must be retained for comparable results.

```bash
SUSS_MLIR_BUILD="$SUSS_MLIR_TOOLS/build"
cmake -S experiments/mlir -B "$SUSS_MLIR_BUILD" -G 'Unix Makefiles' \
  -DMLIR_DIR="$SUSS_MLIR_SDK/lib/cmake/mlir" \
  -DLLVM_DIR="$SUSS_MLIR_SDK/lib/cmake/llvm" \
  -DCMAKE_C_COMPILER="$SUSS_MLIR_SDK/bin/clang" \
  -DCMAKE_CXX_COMPILER="$SUSS_MLIR_SDK/bin/clang++" \
  -DCMAKE_EXE_LINKER_FLAGS="-fuse-ld=$SUSS_MLIR_SDK/bin/ld64.lld" \
  -DCMAKE_OSX_SYSROOT="$(xcrun --show-sdk-path)" -DCMAKE_BUILD_TYPE=
cmake --build "$SUSS_MLIR_BUILD" --target suss-mlir-opt suss-mlir-export --parallel 1
ctest --test-dir "$SUSS_MLIR_BUILD" --output-on-failure --parallel 1
SUSS_MLIR_OPT="$SUSS_MLIR_BUILD/suss-mlir-opt"
SUSS_MLIR_EXPORT="$SUSS_MLIR_BUILD/suss-mlir-export"
python3 experiments/mlir/fixtures/check.py "$SUSS_MLIR_OPT"
python3 experiments/mlir/fixtures/check-analysis-globals.py "$SUSS_MLIR_OPT"
python3 experiments/mlir/bridge/export-fixtures/check-export.py "$SUSS_MLIR_EXPORT"
python3 experiments/mlir/bridge/effects-fixtures/check-effects.py "$SUSS_MLIR_EXPORT" "$SUSS_MLIR_OPT"
```

AppleClang 21 could compile this slice but failed to link LLVM 23 bitcode archives;
use bundled Clang 23 and ld64.lld. CTest covers dialect/export/effects boundaries,
not Wasmtime execution. The effects checker regenerates deterministic fixture
JSON and verifies unchanged-output sentinels on rejection.

## Isolated Rust and actual execution

```bash
SUSS_MLIR_TARGET="$SUSS_MLIR_TOOLS/rust-target"
export CARGO_TARGET_DIR="$SUSS_MLIR_TARGET"
CARGO_BUILD_JOBS=2 cargo test --manifest-path experiments/mlir/bridge/Cargo.toml --locked -- --test-threads=2
CARGO_BUILD_JOBS=2 cargo build --manifest-path experiments/mlir/bridge/Cargo.toml --locked
SUSS_MLIR_BRIDGE="$SUSS_MLIR_TARGET/debug/suss-mlir-bridge-draft"
python3 experiments/mlir/fixtures/check-native-analysis.py "$SUSS_MLIR_BRIDGE" "$SUSS_MLIR_OPT"
python3 experiments/mlir/bridge/source-fixtures/check-source-export.py "$SUSS_MLIR_BRIDGE" "$SUSS_MLIR_EXPORT" --execute
python3 experiments/mlir/bridge/effects-fixtures/check-native.py "$SUSS_MLIR_BRIDGE"
python3 experiments/mlir/scripts/measure-pipelines.py "$SUSS_MLIR_BRIDGE" "$SUSS_MLIR_EXPORT" > "$SUSS_MLIR_TOOLS/pipeline-comparison.json"
python3 experiments/mlir/scripts/measure-implementation.py > "$SUSS_MLIR_TOOLS/implementation-footprint.json"
```

The inner workspace lock is independent of the root lock. Add `--offline` only
when its complete pinned dependency cache is available. V2 execution checks
complete genuine reanalysis, representative correspondence, section resealing,
artifact identity, ABI gates, rooted cross-fragment closure/GC and exact bits.
The numeric specialization assumptions are in `bridge/source-schema.md`.
Measurements execute all 92 pairs outside emission timings; the report retains
samples, hashes, profiles and exclusions. These are not production speed claims.

## Fresh pinned reference observations

Set `SUSS_CLJS` to a clean source checkout at
`c4295f303100bbf5afac449242d30bca1126f1a1`. Oracle drivers verify that pin and use
Clojure 1.12.1, separate output directories and a 512 MiB heap. The development
Maven cache is `/tmp/suss-oracle-m2`; dependencies must be available or fetchable.
Effects require Node v24.5.0 at `/opt/homebrew/bin/node` on this tested platform.
Build the existing Suss CLI in a separate target after the isolated graph exits.

```bash
SUSS_NATIVE_TARGET="$SUSS_MLIR_TOOLS/native-target"
CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR="$SUSS_NATIVE_TARGET" cargo build -p suss-cli --bin suss --locked
python3 experiments/mlir/bridge/source-fixtures/check-source-oracles.py \
  --upstream "$SUSS_CLJS" --workdir "$SUSS_MLIR_TOOLS/source-oracle" \
  --suss "$SUSS_NATIVE_TARGET/debug/suss"
python3 experiments/mlir/bridge/effects-fixtures/run-cljs.py \
  --upstream "$SUSS_CLJS" --workdir "$SUSS_MLIR_TOOLS/effects-oracle" \
  --native "$SUSS_MLIR_BRIDGE"
```

Source CLI printed 9/13 is separate from independent boxed binary64 decoding.
Frozen expectations are compared, not rewritten to accommodate mismatches.

## Final-head gates and CI coverage

Root CI runs the existing workspace baseline. It does **not** run this isolated
workspace or acquire LLVM/C++ tools. Require retained explicit experiment gates
on the committed source alongside green final-head root CI and independent review.
Record base commit, experiment content identities, bootstrap identities, tool
versions/profiles, commands and statuses in the evidence manifest. The full root
baseline remains `cargo test --workspace --locked -- --test-threads=2`.
Compiler adoption requires a separate architectural decision; this evaluation
recommends deferral and does not advance M5–M7.
