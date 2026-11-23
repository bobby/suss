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
