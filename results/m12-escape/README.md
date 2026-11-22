# Local closure scalar storage

The compiler now removes a fixed-arity closure when its complete use set is
calls with the correct arity. Its body moves into those call sites after source
overload, primitive return and recur checks. Captures and already evaluated
arguments enter fresh CFG parameters, keeping declared IR types while physical
long/double storage propagates through the edges. Returning paths join at the
original call result; always-throwing paths remove their unreachable suffix and
unused SSA records. Escaped closures retain their original representation.

Single-use bodies move. Repeated sites share a growth allowance measured from
the original graph, so copying cannot grow without a source-relative bound.
Metering covers scans, owned literal data, copying and CFG updates. Normalized
async continuation graphs/spill plans are left intact. General, variadic,
self-recursive and closure values flowing through CFG edges remain conservative;
their value behavior is included in the controls, not claimed as eliminated.
This change adds no public function or common runtime ABI layout.

`oracle.clj` freshly asserts30 JVM Clojure1.12.6 values. The same cases execute
in Runtime, Macro and actual AOT in `clojure_nonescaping_closures.rs`. They cover
wide long/double captures, source Object casts after calls, float bounds, NaN,
signed zero, overflow, argument/body effect order, checked primitive parameters,
primitive returns, loop recurrence, arity errors, unknown-function/collection
escape, nested/returned closures, observed functions, mixed/variadic/named
arities, mixed numeric categories and exception/finally behavior.

`codegen-before.log` is an actual failing allocation regression after execution
already confirmed the expected value. With the pass, the exact inspected loop
component executes1,234,591 iterations, matches `codegen-oracle.clj` on JVM, and
has no normal-path calls or loop-carried boxes. `third.log` records the initial
20-value fixture, eight macro fuel checks and13 code-generation tests passing.
The expanded final fixture additionally executes an always-throwing local body,
including the unreachable-code/SSA cleanup that its initial failure required.

`measurement-summary.json` retains the exact before/after executables and
components, two null-GC RSS slopes at1,000,000/2,000,000 iterations, and user-space
perf instruction deltas at100,000/200,000. All numerical results are asserted.
The closure falls from119.5–119.8 estimated bytes/iteration to below RSS resolution
and from1777.5 to28.0 instructions/iteration. The direct control remains below
RSS resolution at about28 instructions. Vector and MapEntry cases remain about
168 and136 bytes/iteration and1629/5579 instructions: their elimination is not
established by this change. These are allocation/instruction observations, not
quiet walltime acceptance. Small-count zero slopes are retained separately;
startup peak memory masked those samples, so larger counts were required.

The four cases are added to `experiments/numeric-representation/suss-jvm/bench.py`.
The JVM MapEntry factory has the same two sequence values as Suss's Java-free
constructor with its cache field. `benchmark-oracle.clj` checks all four official
loop sources at three counts each (12 JVM assertions), including the two RSS
counts. `measure-before.py` requires the separately built/frozen baseline
executable; `measure-after.py` checks the recorded source before measuring.

Raw unsuccessful attempts remain:

- `first.log`/`first-escape.rs`: missing arithmetic operand remapping.
- `second.log`/`second-escape.rs`: the removed closure's nil placeholder had the
  wrong IR result type. Fresh CFG parameters also preserve nested capture types.
- `pre-prune-fix-*`: the expanded always-throwing body exposed unused SSA records
  after unreachable blocks were removed. The later pass compacts those records.
- `expanded-oracle-first.*`: throwing a keyword on JVM produces a ClassCastException,
  so the portable payload check now throws ExceptionInfo and checks its message.
- `expanded-oracle-second.*`: JVM has no `long?`; the numeric-category test uses
  portable `integer?` and `float?` instead.
- `prune-fixed.log` and `ifn-unsupported-*`: Suss's `ifn?` is unresolved. The
  supported observation control uses `fn?`; no `ifn?` support is inferred.
- `verifier-fixture-failure.*`: an existing malformed-IR test assumed an immediately
  called closure still existed. Returning that closure now keeps it escaping;
  all seven original mutation checks remain, with no verifier criterion relaxed.

The vector redefinition probes and JVM oracle both return3 when nth or vector
is redefined around literal-vector destructuring. They are preparation for the
remaining aggregate work. A separate preliminary analyzer probe reports unresolved
`letfn`; that example requires explicit support rather than a passing/skip claim.
No complete collection/closure escape or numeric-tower coverage follows here.

`source-inputs.json`, `source.patch` and `source-check.py` reconstruct six exact
production/test/benchmark inputs from the recorded base. The original five
measured inputs and executable remain byte-identical after the additional verifier
fixture change. Gate commands, return codes and logs are retained separately.

Final gates pass144 tests across28 focused targets, zero failed/ignored, including
record/REPL defaults, capture/arity/receiver tests, source hints and async execution.
Java-free bootstrap reproduction passes both phases byte-exact plus4 tests;
actual Binaryen133 passes2. The reviewed-suite comparison passes1 in156.59s,
10 unrelated tests filtered, with all2758 assertion observations and known-failure
records unchanged from the preceding capture. Complete compressed observations,
commands and hashes are retained. A current unfiltered full-workspace result and
new quiet timing acceptance are not established by these gates.
