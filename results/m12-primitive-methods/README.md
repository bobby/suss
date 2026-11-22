# Primitive method source types

The compiler checks primitive returns with lexical capture facts and rejects
nonprimitive/mismatched recurrence operands for source primitive parameters.
The retained analyzer body precedes physical parameter and return conversions.
Checks include the first loop parsing context before boxing/reanalysis.

`oracle.clj` asserts17 values and15 diagnostic substrings on JVM Clojure1.12.6;
`oracle-command.json` and `oracle.log` retain the invocation and actual output.
`clojure_primitive_method_types.rs` executes those same values in Runtime, Macro
and actual AOT and checks all diagnostic cases through those routes. Cases cover
nested parameter captures, named arities, primitive widening, hinted Objects,
reflective bitwise results, ordinary loop boxing and return-only hints.
`compiled_primitive_params.rs` also executes WIT-only capture/recur controls:
WIT storage conversion cannot invent a source primitive-return/recur error.
No Java implementation is copied or shipped.

`before-fixture.rs`, `before.json` and `before.log` retain the original smaller
9-value/11-diagnostic regression against production revision ea7af731 and its
recorded compiler hash. All four tests fail: a captured primitive return is
incorrectly accepted and a captured reflective bitwise return is incorrectly
rejected. The first failing assertion in each test does not establish that every
case failed. Additional raw exploratory probes are retained separately; the
corrected probe removes trailing parenthesis suffixes accepted by read-string.
The final oracle and native fixture contain balanced, identical source strings.

`source-inputs.json` and `source.patch` identify the exact five production/test
inputs from the recorded base. `python3 results/m12-primitive-methods/source-check.py`
reconstructs and compares every byte; an optional revision verifies published
source. Documentation and evidence changes are outside that measured patch.
`commands.json` retains final gate commands, exit codes and durations, with raw
logs alongside. Use stable Rust, two Cargo jobs/test threads, the isolated
m12-hints-target, outer target/suss cache and MemoryMax4G. Binaryen uses actual
version133, and bootstrap verification excludes Java/Node from PATH.

These checks do not establish complete numeric-tower support, all raw core
results, all escape elimination, a new quiet performance ratio or a current
full-workspace baseline. The reviewed vendored-suite comparison, when passing,
preserves known failures rather than claiming complete upstream conformance.

Final gates:64 focused tests across12 targets,4 bootstrap execution tests with
byte-exact two-phase reproduction,2 actual Binaryen133 optimization tests, and
1 reviewed-suite comparison pass. Zero failed/ignored; the suite command filters
10 unrelated harness tests. The complete suite observations are compressed with
both compressed and uncompressed hashes in receipts.json.
