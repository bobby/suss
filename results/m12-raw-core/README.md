# Raw numeric core results

Original Wasm lowering keeps known quot/rem/abs and unchecked arithmetic results
in raw long/double locals, including guarded primitive hits. Direct source calls
inline independently of local-hint metadata; ordinary aliases retain function
calls. Error branches invoke the original runtime helper with the original
operands. Long MIN/-1 quotient wraps, unchecked longs wrap, double quotient
truncates through a long only in range, and remainder retains quotient/subtraction
semantics including signed zero and infinite divisors. Numeric helpers check
operands so a direct boxed inline call raises the same language error as public
core rather than trapping on an invalid representation cast.

`oracle.clj` asserts22 fresh JVM Clojure1.12.6 values. The CLI fixture executes the
same source values in Runtime, Macro and actual AOT. Four additional Suss-only
cases follow ADR-0004's ordinary-IFn fallback: replacement doubles, longs,
non-numbers and signed zero retain their actual values. They are not JVM value
matches: the separate JVM reference records primitive interface-cast failures.
`rebind` retains the actual frozen-before/current components' input source/WIT,
compiler/component hashes, compile/run logs and four checked exports per compiler.
Large component binaries remain under target/probe rather than in Git.

`portable_unboxed_locals.rs` adds long and double loops which inspect and execute
the exact same compiled component bytes. They require raw division/remainder
instructions and permit helper calls/boxes only on throwing paths or final
returns. Both million-iteration loop values also pass fresh JVM assertions in
`codegen-oracle.clj`. These are code/execution checks, not quiet timing evidence.

`before.json`, `before.log`, `codegen-before.log` and the retained original test
fixtures identify the negative controls on production4e3c4acf:two CLI tests fail
the direct/aliased redefinition distinction, and two code-generation tests fail
the raw opcode requirement. The first implementation's log uses the original
22-value fixture, before the four Suss-only cases were added.

`source-inputs.json`, `source.patch` and `source-check.py` reconstruct all seven
production/test inputs byte-for-byte from the exact base. An optional revision
argument verifies published bytes. `commands.json` and raw gate logs identify
final commands, exits and durations. Builds use stable Rust, two Cargo jobs/test
threads, MemoryMax4G, m12-hints-target and outer target/suss. Binaryen uses actual
version133; bootstrap reproduction excludes Java/Node from PATH. No Java code is
copied or shipped; existing core recipes retain their upstream EPL provenance.

No complete numeric-tower, all-core-result, full escape-elimination, new quiet
benchmark or current full-workspace acceptance is inferred. The reviewed suite
comparison retains known failures rather than claiming full conformance.

Final gates:68 focused tests across13 targets; byte-exact Java-free bootstrap
reproduction plus4 execution tests;2 actual Binaryen133 optimization tests; and
1 reviewed-suite comparison pass. Zero failed/ignored; the suite command filters
10 unrelated harness tests. All gate logs and complete compressed observations
have retained hashes in receipts.json.
