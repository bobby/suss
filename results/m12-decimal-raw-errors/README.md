# Raw numeric diagnostic and combined constructor evidence

CI45116208050 at8b70a31f has one failing test: the unchanged
`raw_numeric_helpers_return_scalars_without_numeric_boxes` guard. Its full failed
job137499533577 log is retained under `m12-decimal-conversions`. All seven other
integration partitions pass. The guard inspects all37 scalar-returning exports,
discovers numeric box layouts from their actual constructors, and forbids both
direct boxing calls and numeric StructNew instructions, including cold paths.

The new Decimal checked-cast error path constructed a temporary Double solely
to format its doubleValue range diagnostic. `before-raw-box-guard.log` reproduces
the failure locally. The repair factors the existing raw f64 formatter into
`double-string`, keeping private scratch reservation, stack reset,32-byte bound,
UTF-16 copy and typed allocation failure. Both ordinary coercion and the raw
error path format the f64 directly; no numeric wrapper is created. Original
helper/result/box-count assertions are unchanged. `after-raw-box-guard.log`
records the unchanged test passing on the actual validated runtime module.

The prior unfiltered local command on8b70a31f was deliberately stopped after
this definitive CI failure, before changing source. Its terminal receipt has
exit-2 after1943s and unchanged inputs; all actual output remains in
`m12-decimal-conversions/workspace-baseline`. It is interrupted evidence, never
a workspace success. `prior-workspace-interruption.json` records the reason
and exact dedicated active systemd scope interrupted with SIGINT. A fresh full
baseline is required on the repaired source.

The separately validated MathContext constructor checkpoint is integrated here;
`../m12-context-constructors` retains its original71 mismatches/phase, all102
fresh JVM controls passing Runtime/Macro/actual AOT,18 cold standalone-CLI
controls and complete65536-code-unit digit mapping. Original source/fixture
hashes qualify each run; constructor fixtures are not replaced by smaller sets.

`gates/commands.json` records seven complete passing groups with frozen source
unchanged:13 unboxed-local tests,4 numeric-format/runtime tests,68 Decimal/
constructor/ABI/decoder/inference tests, the unchanged AOT size test,4 Java/
Node-free bootstrap execution/reproduction tests,15 foundation commands
(364 Python tests), and compilation of all workspace test targets. The AOT size
guard remains3895 <=3900 bytes/form, with96 <=128 extra split-form code bytes.
These90 tests do not establish a full workspace or final-head CI pass.

`run-suite.py suite-after-raw-strings` returns0 with frozen inputs unchanged;
`audit-suite.py` proves zero changed assertions, tests or namespace outcomes.
The entire4585-row observation payload is byte-identical to the preceding
capture (SHA256 c7e194aa5bf52ff652ab1bde5e0b69dee9d9cd7727705da8439c1bd6a5a846a9).
Counts remain4326 host matches /63 guest /208 failures /2342 unexecuted /84
failed namespaces. Primary values, capture routing, count and skip guards,
and the reviewed failure baseline are unchanged. The remaining54 Decimal
source-form candidates and six unexecuted Decimal-operand assertions remain;
these repairs do not establish complete BigDecimal suite acceptance.

The fresh required full baseline subsequently completes on a12b2796: the exact
`cargo test --workspace --locked -- --test-threads=2` command exits0 after9056s,
with2395 passed, zero failed and14 existing ignored tests. All3852 inputs remain
unchanged. `workspace-baseline/command.json.gz` and `run.log.gz` preserve terminal
receipts/output; their raw SHA256 values are indexed in `archives.json`.
`workspace-baseline/qualification.json` lists all changes to0436ab30, whose
production source is identical and whose added suite profile/regression has
separate passing CI45125078838. This is not a local workspace run of0436ab30.
Remaining known Decimal failures/unexecuted assertions are not treated as passing.
