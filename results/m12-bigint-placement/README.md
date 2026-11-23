# BigInt magnitude placement probe (#338)

This development probe compares original little-endian u32 magnitude addition
and schoolbook multiplication in GC arrays with the same arithmetic in an
allocator-free Rust helper. It is a required placement measurement, not shipped
BigInt/Ratio support. JVM Clojure1.12.6 checks146 exact arithmetic results
for73 operand pairs. All876 complete candidate results match independent
integer arithmetic, normalized length and sink checksum in both auto/null GC.
Zero, carry chains, uneven sizes and16384-bit operands are included.

The helper started as a read-only snapshot of the existing local experiment;
prototype-inputs.json preserves its input hashes. The original worktree was not
modified. Both kernels were then given the same short-outer-loop multiplication
policy. No upstream implementation body is copied. Helper Rust1.98.0 matches
runtime/numeric's pinned toolchain and size-oriented release profile. The host
uses the actual workspace Wasmtime51.0.0-rc.1, not the older standalone
numbench49 dependency. There is no shipped JVM dependency.

The bridge copies both GC operands into aligned scratch, checks memory capacity,
resets the Rust stack, calls the helper and copies the complete capacity back
into one GC result array. Operands remain unchanged. Native GC and bridged
results have the same array capacity; both retain a logical normalized length.
The raw linear-memory helper is a separate lower bound, not a candidate value
representation. Sign handling, dispatch and a common BigInt wrapper are outside
this magnitude-only probe. It uses fixed input bounds and does not expose the
future arbitrary-precision language interface.

The helper and GC code are merged with Binaryen133 into one import-free module,
so the calls model the existing embedded numeric helper rather than host calls.
Custom descriptors/exact refs are disabled, matching the actual runtime feature
profile. The first all-features assembly produced exact refs unsupported by the
host; its errors and the unsuccessful host-feature attempt remain. The final
module executes rather than merely passing an encoding check.

## Reproduction

Use a worktree containing source-inputs.json's inputs, with a Binaryen133 bin
folder in SUSS_BINARYEN_BIN if the recorded local default is unavailable:

    python3 experiments/numeric-representation/bigint-placement-build.py
    cargo +stable build --locked --profile test -p suss-compile --example bigint_placement
    python3 experiments/numeric-representation/bigint-placement-verify.py
    python3 experiments/numeric-representation/bigint-placement-oracle.py
    python3 experiments/numeric-representation/bigint-placement-measure.py

SUSS_BIGINT_PROBE can select the compiled host executable. The default is the
isolated m12-bigint-target used here. Builds preserve developer flags; no
RUSTFLAGS are set. Native serialized code is produced locally from this trusted
source-hashed module by prepare and must never be supplied from another source.
Raw commands, module/helper/host hashes, JVM JAR hashes and results remain.

## Measured result

The36 operation/shape cases each have two instruction-slope pairs for five
candidates/controls:720 asserted perf samples. instructions.csv reports both
with-control and net counts, plus the observed spread. All shapes use the same
operand values for competing paths. No quiet walltime claim is made.

Representative with-control instructions per operation:

| Operation and limbs | GC | Helper including GC copies |
| --- | ---: | ---: |
| add1×1 |369 |513 |
| add32×32 |2983 |4026 |
| add512×512 |43456 |58437 |
| multiply2×2 |645 |805 |
| multiply3×3 |1034 |1058 |
| multiply4×4 |1556 |1349 |
| multiply32×32 |69796 |25870 |
| multiply512×512 |17333562 |5323318 |
| multiply1×512 |34723 |43124 |
| multiply2×512 |68558 |53439 |

These data favor GC addition, small GC multiplication and a helper for larger
balanced products. The1×512/2×512 distinction rules out dispatch based only on
the larger operand. A production crossover still needs validation with actual
value wrappers and dispatch; division/GCD are not measured by these two kernels.

Persistent magnitude storage remains a GC array on both paths, avoiding linear
memory lifetimes tied to GC finalizers. The44 null-GC samples measure one dense
array of the same result capacity. Representative slopes are about24 bytes at
capacity2/3,48 at9,144 at33 and4110 at1024; bytes.csv retains both estimates.
The wrapper's common bytes are excluded. allocate writes nonzero elements so
payload pages are committed, and each run asserts its checksum.

The initial wait4 peak-RSS sampler inherited the larger Python controller's
high-water floor, making small arrays appear cheaper than their contents.
Those unselected samples remain in inherited-rss-measurement.json. The corrected
sampler reads the child's own /proc/self/status VmHWM; it also retains wait4's
value for comparison. Earlier balanced-only samples remain separate from the
final unequal-shape/short-outer-loop measurements. No negative or below-floor
sample is silently recast as zero or used for the placement decision.
