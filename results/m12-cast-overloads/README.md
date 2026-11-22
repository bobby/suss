# Source-selected narrow cast overloads

The 48 assertions in `oracle.clj` execute in fresh JVM Clojure 1.12.6. They
extend the preceding 30-case bitwise/cast matrix with 18 cases covering the six
recorded mismatches, matching controls, negative boundaries, NaN, original error
messages, aliases, hints, expression-context loops and evaluation order. The
same 48 values and four separately labeled Suss ADR-0004 redefinition controls
execute in Runtime, Macro and actual AOT in `clojure_raw_bits_and_casts.rs`.

`before.log` is a real failing run on the source at base128a673e with only the
expanded test added: both native-phase and AOT tests fail on float identity of
infinity. `before-tested.rs` retains that exact test; `before-test.rs` retains
the earlier test before expansion. The preceding receipts in
`../m12-bits-casts/additional-observations.json` independently retain all eight
actual AOT checks at two frozen compiler snapshots, both with six failures and
two matches. Those observations remain unchanged. The new
`additional-observations.json` records all eight passing with the corrected
compiler, plus two further checks. Compiler and component hashes are retained.

The implementation selects from source class before hint consumption:

- Object byte/short/int casts truncate via Number.longValue() before checking
  the narrow range. Byte/short failures print the original operand; int failures
  report integer overflow. Genuine double byte/short casts still reject NaN and
  fractional values outside the original bounds.
- float(float) is an identity for infinities. int(float) compares with the
  float-rounded upper bound 2^31 and saturates there to Integer/MAX_VALUE;
  int(double) retains the original double bound.
- Generic first-class core functions also use the Object modes. Int lowering
  retains its source primitive result kind independently of its selected helper.

`oracle-first.clj`/`.log` preserve an initial incorrect expectation that Object
byte/short errors would print the truncated integer; fresh JVM execution showed
they print the original floating operand. The corrected 48-assertion oracle
passes. `extra-float-probe-first.*` separately retain a speculative expectation
about boxed Float infinity; `extra-float-probe.*` record the actual JVM values.
This is an explicit representation boundary in ADR-0003, Types: Suss has no
Float value type and float returns a double. In the extra infinity vector JVM
returns `[##Inf ##Inf ##Inf]`, while Suss returns `[##Inf :error :error]` after
Object/call boundaries. The second extra AOT check asserts the Suss contract;
it is not a JVM value match. The first extra check (a float local versus its
Object alias in int) matches JVM. No Float box type is introduced.

`codegen.sus` executes 1,234,587 iterations, including physically raw doubles
with Object source types and the float-to-int upper boundary. Fresh JVM and
execution of the exact inspected component both return5302551397102356. The
artifact regression rejects normal-path calls and loop-carried boxes. The
library regression checks all37 raw helper exports for exact scalar return
types and absence of numeric boxer calls/struct construction, including the
integer result of the new int-float helper.

`source-check.py`, `source.patch` and `source-inputs.json` reconstruct all nine
changed production/test inputs from the recorded base and compare exact bytes.
`first.log` passes2 value tests and11 pre-existing code-generation tests; the
final focused gate also includes the new loop and stricter helper signatures.
Run `python3 results/m12-cast-overloads/run-gates.py` from the probe worktree;
`commands.json` records actual commands, exit codes and durations. The runner
uses stable Rust, two jobs/threads, MemoryMax4G, the shared m12-hints target,
outer native cache, and actual Binaryen133. No RUSTFLAGS are set.

These observations do not prove complete conversion or numeric-tower coverage,
collection/closure escape elimination, a current full-workspace baseline or a
new quiet benchmark ratio. No Java implementation is copied or shipped.

Final source gates:75 focused passes across14 targets; zero failed/ignored.
Java-free bootstrap reproduction passes both phases byte-exact plus4 execution
tests. Actual Binaryen133 passes2. The reviewed-suite comparison passes1 in
210.85s, with10 unrelated harness tests filtered and known failures retained.
Complete compressed observations and hashes are in receipts.json. The
intermediate/ directory preserves the successful earlier gates before final
review limited source-class queries to the affected casts; all four gates were
reexecuted for the final source rather than inferred from that snapshot.
