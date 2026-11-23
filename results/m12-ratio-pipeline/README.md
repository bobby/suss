# Exact results in the raw compiled-fragment observer

`ci-run.json` and the original shard logs identify the failing f9d5f91a run.
Shard 0 exposes two stale pipeline assumptions: `(/ 4)` is expected to be a
Double, and the corpus observer rejects its Ratio layout. Production exact
division already returns the normalized Ratio required by Clojure.

`python3 results/m12-ratio-pipeline/oracle.py` obtains twelve fresh primary
Clojure 1.12.6 observations. The original source, stdout, command, JAR hashes
and generated expected tags remain here. The examples include signed and
wide ratios, reduction, integer division, BigInt zero and a Double control.
The historical ClojureScript corpus is unchanged; its unary-divide case now
has an explicit primary-Clojure override.

The raw fragment observer captures canonical numeric types using the shared
runtime's constructors and reuses the independent decoder's direct field/limb
inspection. It does not print results, convert them to doubles or normalize
them on the host. Unknown or malformed objects still fail. All 397 historical
corpus cases remain, with twelve additional independently observed values.

`python3 results/m12-ratio-pipeline/run.py` runs the entire affected test target
with an isolated Cargo target, two build jobs and test threads, stable Rust
and a 4 GiB scope. `command.json` records the actual terminal result and frozen
tracked inputs; `run.log` retains all test output. Read the receipt before
claiming success. These focused results do not establish a full workspace pass.

Shard 5 independently reports fuel exhaustion in the existing 32,768-unit
quoted-string regression. Its complete failure log remains here. The pipeline
observer repair does not change production fuel, printing or that regression.

The recorded affected-target run terminates0 in635.98s:22 tests pass, zero
fail/ignored, all tracked input hashes unchanged. This includes both adversarial
decoder checks, all397 historical corpus cases and all12 fresh exact values.
