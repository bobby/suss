# Execution-time randomness

Retained Clojure1.12.6 forms, exact adaptations/patches and EPL provenance accompany
126 deterministic multiply/int transformations. The42 separately retained real
JVM calls and16-draw stream are nondeterministic observations, not native goldens.
The typed callback/WASI provider probe is feasibility evidence only.

The production runtime stays import-free and roots its nullable entropy callback
in an appended typed global outside the shared ABI prelude. Native sessions use
operating-system entropy. AOT accepts only an explicitly selected standard random
interface, encoded from the resolved WIT graph with the shared generic encoder;
the lowered provider is installed before source initialization. No provider means
a catchable language error, without a seed, constant or hidden world import.
Both ordinary and optimized AOT assembly preserve draw effects.

The public forms retain numeric multiplication then checked Object int-cast.
Lexical checked function values avoid core's historical coercing arithmetic
macros. Native tests require126 primary transformations per Runtime/Macro phase,
exact provider consumption, GC retention and independent host observation.
Boundary tests execute real ordinary/optimized AOT artifacts and controlled words.
These are requirements, not pass claims before new recorded executions.

Run the recorder itself inside a bounded systemd scope, with an isolated owned
SUSS_RANDOM_TARGET and disk-backed TMPDIR. run-focused.py records the frozen
input hashes, exact native commands, exit codes and terminal input recheck.
Complete original constantly/some-fn bodies, full workspace, bootstrap and actual
CLI/WASI public API coverage remain separate acceptance gates.

At97a2d235, the raw production entropy target returns0. Ordinary selected-WIT
and pure missing-provider AOT cases pass; the optimized case fails because
wasm-opt was not configured, preserving overall101 and unchanged inputs. The
public-core command was not reached. The retained Binaryen133 tools match their
original official archive/tool provenance. Fresh focused runs select the tool
explicitly and run the optional optimization test with --include-ignored. No
optimization pass is inferred from default CI's named ignore.

Public-effects retains16 fresh actual-API primary properties. The new public
target adds126 controlled transforms through one real compiled AOT component
and actual CLI/WASI streams with fresh-instance checks. Provider failure,
replacement recovery and unused draw effects have explicit boundary tests.
These new requirements need separate native receipts. run-public.py uses the
workspace feature graph and captures both complete original bodies.

The1ded898d fixed-draw comparison exposes a fixture overload mismatch at the
upper int fraction. Actual rand-int bytecode calls RT.intCast(Object), whereas
the original literal-only int expression selects the primitive overload. The
separate object-transformations-boxed corpus retains126 fresh Object controls
and a generated JVM bytecode proof with the same multiply(Object,Object) and
intCast(Object) signatures. Exactly one expected value differs:2147483647
rather than an error. Original literals/receipts remain unchanged; the initial
hint-only probe failure is also retained. Public tests select the Object corpus;
production numeric code is unchanged. Fresh native comparison remains required.
