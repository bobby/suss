# Exact numeric behavior and validation

BigInt, normalized Ratio and BigDecimal values use the compiled Runtime and
Macro pipeline, including compiled macro transfer, retained values after GC,
actual AOT and WIT. The reader retains exact values and categories; arithmetic,
promoting operations, hashing, comparisons and precision follow pinned Clojure
1.12.6 and [ADR-0003](../adr/0003-clojure-numeric-tower.md).

Numeric source overloads distinguish primitive calls from ordinary Object
calls. Literal constants belong to their source function: symbolic NaN and
infinity constants share across its arities and compiler-created try/catch,
finally and thread-binding regions. Nested source functions get their own pool;
`with-redefs` introduces such a function. Artifact cache keys preserve the
literal alias partition. These internal markers grant no host or Var authority.

The implementation uses WasmGC for small arithmetic and the no_std helper for
measured larger products. The production dispatch takes the helper only when
both operand lengths are at least two limbs and `(la - 1) * (lb - 1) >= 8`.
The production GC kernel, dispatcher and wrapper match the authenticated measured
source; helper multiply/trim bodies also match. The preserved measurements
contain 854 profile commands, 320 verification runs, 128 instruction pairs,
five allocation pairs and 159 compiled benchmark commands. Byte differences
below RSS resolution remain blank. These are measurements of the recorded
kernels and placement choice, not a fresh whole-compiler performance claim.

The host/Macro/Var ownership contract is documented separately in
[host-macro-boundary.md](host-macro-boundary.md). Numeric operations use those
authenticated cells, Vars and roots. The macro fuel diagnostic change identifies
actual exhaustion during transformer binding lookup; original fuel, effect,
span and recovery behavior remain covered.

## Acceptance map

The source admission preserves the original nine BigInt/Ratio requirements and
six Decimal requirements, their complete text and mapped source/test names.
Every declared test in all 17 mapped targets passes in the current native batch
and the exact workspace run. All 261 original suite case-file Git blobs remain
unchanged. Complete affected source bodies and all 26 independent default-num
assertions remain covered. No primary, comparison, skip, guest, ledger or fuel rule is relaxed.

| Original issue criterion | Required behavior | Acceptance entry points and additional evidence |
| --- | --- | --- |
| #338.1 Literals | BigInt/Ratio reading and printing, overflow integers, removal of the unsupported evaluation diagnostic | `clojure_bigint_integers`, `clojure_bigints`, reader `biginteger_carrier` |
| #338.2 Exact division | Exact integer division, normalized ratios, integral results retain BigInt category | `clojure_ratios`, `clojure_bigint_integers` |
| #338.3 Promotion | Promoting arithmetic and increments/decrements; BigInt results retain their category | `clojure_bigint_integers`, `clojure_exact_inline` |
| #338.4 Functions | Numerator/denominator, rationalize, bigint/biginteger and numeric predicates | `clojure_bigint_integers`, `clojure_ratios`, `clojure_biginteger_carrier` |
| #338.5 Equality/hash/order | Clojure 1.12.6 category equality, hashing, compare and cross-category `==` | `clojure_numeric_classes`, `clojure_exact_inline`, `clojure_ratios`, `clojure_number_source_objects` |
| #338.6 WIT | Exact BigInt for incoming u64 above the signed-long range | `portable_aot_wit_numbers`, including all unsigned-range ingress/egress and checked result controls |
| #338.7 Inline routes | Actual raw-local and boxed fallback coverage for equality/hash/identity/order/bitwise/overflow | `clojure_exact_inline`, `clojure_unboxed_locals`, `clojure_raw_numeric_core`, `clojure_number_source_objects`; emitted raw parameter types are inspected |
| #338.8 Implementation choice | Arbitrary precision selected using instruction and byte measurements, including GC read/copy costs | `bigint_helper`; authenticated historical measurement and current production kernel/dispatcher/wrapper and helper multiply/trim conservation |
| #338.9 Evidence | Original suite numeric assertions and before/after regressions | Complete original capture, exact independent default-num body, numeric classes, raw/boxed/AOT matrices, and retained BigInt/Ratio before observations |
| #339.1 Representation | Arbitrary-precision unscaled integer and signed 32-bit scale | `clojure_decimal_values`, `clojure_decimal_arithmetic`, including independent field decoding after GC |
| #339.2 Reading/printing | M literal scale and printing; str omits suffix | `clojure_decimal_values`, `clojure_decimal_constructors` |
| #339.3 Arithmetic | Scale rules, exact/nonterminating division, math context and rounding modes | `clojure_decimal_arithmetic`, `clojure_division_overloads`, `clojure_decimal_conversions`; source overloads, dynamic context, retained fields and actual AOT |
| #339.4 Equality/hash/order | Decimal equality/hash and cross-category comparison without category conflation | `clojure_decimal_arithmetic`, `clojure_numeric_classes`, `clojure_number_source_objects` |
| #339.5 Functions | bigdec and decimal? | `clojure_decimal_conversions`, `clojure_decimal_arithmetic` |
| #339.6 Evidence | Original suite Decimal assertions and before/after regressions | Complete original capture, independent default-num body, Decimal matrices and actual frozen subtraction/bigdec semantic negative executions |

## Recorded commands and outcomes

The recorded runtime source is `6ceb25674ceea2600c9e2346b78179740cd0a37a`.
The evidence manifest under
`results/m12-num-api/reviewed-core-final-evidence/manifest.json` lists the
unmodified producer receipts and compressed logs with raw and archive hashes.

- The dedicated native controller exits 0 for all 11 commands: nine macro tests,
  222 numeric tests across 33 targets, 143 ownership/transport executions,
  the explicitly included random-effect guard, 24 suite tests, suite build/lock,
  strict capture, Java/Node-free bootstrap and both provenance checks.
- `cargo test --workspace --locked -- --test-threads=2` exits 0 with
  2732 passing executions across 388 result groups
  and 15 existing ignored executions, in 13268.372 seconds.
  All 13,510 input files, HEAD, runner, Binaryen and prerequisite receipts remain
  unchanged through actual termination and archive.
- CI on the same source passes all ten required jobs, with 2,744 actual passing
  executions. Raw job metadata and logs are retained under
  `results/m12-num-api/ci-6ceb2567`. Later evidence-only commits retain their
  source qualification; their final-head CI is recorded separately on GitHub.

Native and workspace captures independently retain all 5,750 ordered rows,
5,638 prior successful decisions, all 229 tagged BigInt/Ratio host matches and
all 200 tagged Decimal host matches. Global counts remain 5,512 host matches,
107 guest judgments, 132 residual failures, 1,196 unexecuted primary rows,
59 failed namespaces and zero skips. This is not language-wide conformance.

The exact committed independent default-num checker passes on the fresh full
capture: all 26 original default rows remain, with 23 host matches and three
explicit existing Short/Byte/Integer class distinctions at ordinals 7/8/9.
Ratio, BigInt and Decimal rows 4/5/6 host-match. The original global JVM/clj
primary has 18 num rows and different class/value profiles; its two `head:and`
failures remain explicit in the unchanged global ledger. The supplemental
lexical audit retains 1,250 candidates: 1,240 host matches, eight existing guest
judgments and those two existing profile differences.

## Negative controls and qualifications

Pinned Clojure 1.12.6 provides independent expected values. Historical BigInt
comparison computes false despite command exit 0; integer division computes a
binary64 approximation, and the Ratio literal invocation fails. A retained
Cargo BigInt failure lacks a separate exit/source-freeze receipt and is qualified
accordingly. Decimal subtraction and bigdec have genuine exit-101 semantic
regressions with frozen input and primary JAR/source/output proofs. Older missing
setup dependencies remain setup failures.

Ten arity controls, nine try/binding/cleanup controls and three root-scope
controls have actual failing Runtime/Macro and AOT regressions on their recorded
older implementations, followed by passing runs on the corrected source.
The final root controls distinguish separate parent/with-redefs constants,
stable constants inside one with-redefs function, and ordinary thread binding's
shared enclosing constants. Original async tests remain unchanged; the removed
exploratory async fixture failed setup and is not semantic evidence.

Interrupted earlier workspace/native runs retain their actual nonzero terminals.
The older native batch's stale-provenance outer failure remains distinct from
its successful component commands. The workspace launcher preflight failure
happened before Cargo and is separately archived; its corrected historical HEAD
proof authenticates the exact archived runner rather than assuming a missing
field is true. These records are not substituted for the successful current runs.

The precise current audits are under
`results/m12-num-api/reviewed-core-final-audits`, including the full capture,
default26 projection/checker, mapped test-name sets, measured-source placement
and before-regression qualifications. Read these together with the actual
implementation and producer logs.
