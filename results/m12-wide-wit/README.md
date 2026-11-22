# Exact unsigned WIT boundaries

The adapter lifts u64 inputs through Long/MAX_VALUE as Long and larger values
as positive BigInt. Scalar, list and option boundaries share this conversion.
Outgoing u64 accepts Long or BigInt in [0, 2^64 - 1], checking the full magnitude
before extracting bits. Other integer boundaries retain their Long rules.
Floating output uses runtime numeric conversion for BigInt and Ratio; direct
Long-to-f32 rounding is preserved. Shared GC layouts remain ABI10.

The component regressions execute actual Wasmtime canonical calls. They cover
low/high category printing, exact overflow promotion, primary hashes, scalar,
list and option round trips, mixed asynchronous parameters, retained values
across GC, unsigned result limits and invalid categories. The original high
u64 regression passes after the implementation (focused-command.json/log).
Additional cases and the complete adapter/ABI/bootstrap gates run separately
through run-gates.py; only terminal commands.json entries establish results.
The expanded target is not certified by the earlier one-test run.

oracle.clj/log and oracle-command.json retain fresh JVM Clojure1.12.6 values,
hashes and float bits with source/JAR hashes. No JVM is a shipped dependency.
before-tests.rs reproduces the original regression source; its SHA256 matches
the frozen before receipts. At5ee8a0d7, the high-u64 test exits101 on a list
containing u64::MAX, and floating output exits101 on the BigInt-to-f64 cast.
before-u64.log is the retained terminal output fragment; before-floats.log
contains the complete independent binary invocation. The tests are later
expanded without changing the production fix tested by the focused run.

The first expanded run contains a fixture mistake: option outputs returned bare
numbers instead of [:some value]. float-failure.log retains the isolated
terminal101 at ratio-option, after the scalar/list conversions passed. The
initial-option-* files retain those fixtures, source hashes, partial target log
and SIGINT exit. The interrupted target is not a passing result, and subsequent
targets did not run. The corrected run wraps payloads in the existing option
representation and preserves all range assertions.

The separate workspace retry at d7d2919b predates the WIT production changes;
it cannot certify them. Production arbitrary-precision helper placement,
complete numeric API/suite coverage and BigDecimal are separate requirements.
