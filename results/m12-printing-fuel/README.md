# Quoted-string work and exact numeric operands

The original f9d5f91a long-string regression fails locally as it does in CI.
`parent-test.json` identifies the actual test executable and its terminal101
result; `parent-test.log` retains the fuel trap. The input contains32,768 UTF-16
units, including quote, backslash, newline, lambda and unpaired surrogates.

`probe.rs` invokes the actual shipped public printer using the same FormBridge
input path, validates every input/output unit and collects before observation.
`profile.py` records compiler/reader/executable hashes, matching Cargo dependency
fingerprints, commands and production input hashes. The one-billion allowance
is a development measurement only; the same program separately validates the
original100M allowance. These are Wasm fuel counts, sampled during ordinary
native inspection, not hardware instruction or quiet-time measurements.

The baseline exact-input profile measures100227864 fuel cold and100824706 on
the repeated long call, in each phase. It retains both100M traps. The initial
literal/warmup probe produced a misleading99.6M estimate; its source and complete
results remain as `initial-literal-warmup-*`. A runtime-injected candidate could
not resolve a private core slot; that failure is retained and proves no runtime
result. All selected-path observations use a rebuilt shipped bootstrap instead.

Computing the next index once alone made the original test pass, but left the
repeated call over100M. `single-index.patch` reconstructs that intermediate
source from dee07958, and `single-index-*` retains its passing scoped gates and
the additional probe failure. It is not the selected implementation.

The final traversal computes each next index once and avoids substring/table
work for code units above92, the largest key in the unchanged complete seven-key
escape table. Every other code unit is copied verbatim; all seven escapes still
use the retained table. The final exact-input profile measures83364722 fuel
cold and83961474 repeated, in Runtime and Macro. Every count at both budgets
validates exact output after GC, including empty strings and unpaired UTF-16.

`python3 results/m12-printing-fuel/run-gates.py` passes all four commands with
frozen tracked inputs:4 chunk-printer tests,45 neighboring collection/options/
object/provenance tests,4 bootstrap tests with Java/Node-free byte reproduction,
and generated-manifest verification. No production or test fuel limit changes.
The final profile command is:

```
SUSS_PROFILE_LIBRARY=libsuss_compile-2874475a5bbbffec.rlib python3 results/m12-printing-fuel/profile.py
```

That library filename belongs to the recorded isolated build; the environment
override selects a caller's corresponding built compiler library when rebuilding.

Separately, the unchanged f9 workspace run stops101 after2845.76s at stale
numeric-operand expectations, after566 passes in133 completed targets and two
passes in the failed target. `results/m12-helper-workspace-f9` preserves its
complete source-qualified receipt/log. Later targets were not reached by it.
Unary integer division is now expected to be a Ratio; `=` rejects the Double
category while `==` compares across categories. The first-class Double control
stays explicit. Fresh JVM1.12.6 source/stdout/JAR hashes confirm these values.
The entire numeric_operand_types target passes3 tests, zero failed/ignored.

These scoped results do not establish the full workspace result or complete
BigInt/Ratio, Decimal or boxing acceptance. GitHub owns issue acceptance.
