# Production BigInt multiplication bridge

The allocator-free pinned numeric helper includes the original schoolbook
magnitude kernel. The production bridge reserves checked aligned scratch,
copies logical operand limbs, resets the Rust stack, and copies the complete
result capacity into GC storage. Published operands remain immutable; no
linear-memory ownership escapes. Addition remains in GC. Division/GCD retain
the GC reference implementation and are not measured by this probe.

The first dispatch policy uses the placement probe's conservative product-area
threshold16 with both operands at least2 limbs. It is provisional: the actual
wrapper data below identify small products for refinement, and the GC loop
still rereads its outer limb. Refine that loop and remeasure the crossover
before claiming a final placement policy or numeric acceptance.

Fresh JVM Clojure1.12.6 and independent Python agree on17 magnitude products,
including zero, carry chains, unequal shapes and512-limb operands. The focused
production test checks204 signed observations through GC, helper and dispatched
paths, exact normalization/display/capacity, poisoned unused input capacity,
scratch overwrite by float formatting, GC retention and input immutability.
The allocation-failure test preserves the language error and operands.
Both tests pass. ABI controls pass63 tests; numeric CLI controls pass11 tests,
including Runtime/Macro and actual AOT primary matrices. Their frozen input
hashes and terminal logs remain in focused-command.json and controls.json.

The control driver then fails its numeric rebuild check because its inherited
RUSTUP_TOOLCHAIN=stable selects1.99.1 instead of the helper's pinned1.98.0.
This is retained as a failure. Bootstrap did not run in that invocation. Use
`env -u RUSTUP_TOOLCHAIN python3 scripts/numeric_runtime.py --rebuild-check`
for the pinned check; do not treat the earlier controls as a bootstrap result.

profile.py/BigInt runtime example profile the actual production wrappers in
workspace Wasmtime with its default optimizing compiler. The locally serialized
modules are source/byte hashed and never external inputs. All250 complete
observations pass across auto/null GC: three multiplication paths plus the
operand control and full BigInt/array allocation control, over25 shapes/cases.
Every invocation independently checks all output fields/limbs and its checksum.
The400 perf samples retain both instruction-slope pairs and raw commands.
The20 allocation runs use the child's own peak RSS under null GC. Full values
cost approximately40 +4*capacity bytes, with both observed slopes retained.
No walltime claim is made, and no below-floor or negative slope is clamped.

Representative instructions per operation including the control:

| Shape | GC | Helper | Dispatched |
| --- | ---: | ---: | ---: |
|4x4 |2630 |2197 |2266 |
|32x128 |366917 |98722 |98809 |
|512x512 |23119505 |5356754 |5356409 |

The initial profiling driver imported bigint-new with eqref instead of its
concrete nullable limb-array parameter. Its executed link failure, original
source and hashes remain under initial-profile-*; the corrected driver is what
profile.json/instructions.csv/bytes.csv certify.

Final-head CI at a21a715d also exposes record membership fuel992401/1003928;
the latter violates the existing strict one-million gate. records-before.*
reproduces those exact costs using the physical CLI executable, retaining its
hash, source and streams. The test/gate must remain intact. Numeric scalar
fallbacks currently repeat numeric type checks before private identity
fallback; remove redundant checks and execute this regression before claiming
that CI repair. The reviewed numeric-suite baseline and Decimal limitations
from earlier receipts still apply.
