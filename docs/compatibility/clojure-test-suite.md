# jank clojure-test-suite conformance corpus

The vendored [jank-lang/clojure-test-suite](../../vendor/clojure-test-suite/README.md)
is the function-level basis of the conformance corpus (issue #211). Its 248 test
namespaces cover about 220 `cljs.core`, `clojure.string` and `clojure.edn` vars,
one file per function, with arity, edge-case and exception assertions. The
pinned ClojureScript oracle decides what each assertion observes; Suss is judged
host-side against those observations. The suite's own `clojure.test` verdicts
are never evidence for Suss. The targeted differential corpora (effects order,
binary64 bits, UTF-16, reader) remain the precision layer.

## Oracle

`python3 scripts/clojure_test_suite.py oracle` compiles the suite with the pinned
ClojureScript 1.12.134 submodule (`cljs.main`, not shadow-cljs) and runs it in Node.
It applies `vendor/clojure-test-suite/patches/0001-*.patch` to a generated copy
only, and fails unless the result equals the reviewed
`tests/oracle/clojure-test-suite-observations.json`; `--write` records it for review.
Java and Node are development dependencies; ordinary CI uses the checked-in file.

[The runner](../../tests/oracle/src/suss_oracle/clojure_test_suite.cljs) replaces
the `cljs.test` reporter. Each assertion becomes one record with a stable ID
`<namespace>/<test>#<ordinal>`, its form, source position, testing contexts,
kind and observations:

| Kind | `cljs.test` path | Observation |
| --- | --- | --- |
| `predicate` | head resolves to a function var (`assert-predicate`) | tagged operand values |
| `value` | any other form (`assert-any`) | tagged value |
| `thrown` | `p/thrown?` (the suite's portable throw assertion) | whether it threw |
| `error` | unexpected exception | thrown value and message |

Values use the transport tags of `suss-oracle.main` (binary64 bits, UTF-16 code
units, collection kinds). Observation never changes the program: a sequence with
an unrealized tail is recorded as `opaque` instead of being forced, and values
the transport cannot represent (functions, atoms, UUIDs, JavaScript objects) or
that exceed a 20,000-node budget are `opaque`. `Math.random` is replaced by a
seeded generator and the suite runs under four seeds. Assertions whose
observations differ are rejected unless they are in the reviewed
`RANDOMIZED` list (random-number tests); those keep only kind and verdict.
Asynchronous tests (`taps`) complete before output is written.

At `95d4a91` the oracle records 231 tests and **5,834 assertions, 0 failures,
0 errors**; 52 are randomized (verdict only) and 139 operands are opaque. The
20 oracle skips are the suite's `when-var-exists` skips of vars ClojureScript
lacks: `bigdec`, `bigint`, `bound-fn`, `bound-fn*`, `decimal?`, `denominator`,
`format`, `intern`, `num`, `numerator`, `+'`, `*'`, `ratio?`, `rational?`,
`rationalize`, `with-precision`, and `future`/`promise` (twice each, inside
`realized?` tests). All are JVM-only APIs outside the portable contract.

## Suss harness

`cargo test -p suss-compile --test clojure_test_suite` loads every suite
namespace, in the oracle's order, through the production `Session` and
`CompiledMacros` with the shipped compiled core. It provides a harness
`clojure.test` replacement in
[`tests/clojure-test-suite/suss`](../../tests/clojure-test-suite/suss): `is`
evaluates predicate operands once, left to right, then applies the predicate, and
records raw values; the host decodes them with the independent test decoder
(`tests/support/portable_decode.rs`). Records are decoded in bulk and, when that
fails, element by element, so one undecodable operand is opaque on its own.
Guest equality never decides a result.

`scripts/clojure_test_suite.py compare` decides each oracle assertion:

* Pass: same kind and a passing verdict, and every deterministic oracle operand
  matches the Suss operand with `oracle_compare.matches` (exact binary64 bits
  apart from the canonical NaN sign, distinct collection kinds, unordered maps and
  sets). A predicate the oracle reports by value is compared by its result.
* Verdict only (`guest-judged`): randomized assertions and opaque oracle
  operands. An opaque Suss operand where the oracle has a value is a failure.
* Fail: wrong verdict, kind or observation; unexpected assertions.
* Not executed: the namespace failed to load, the test failed outside an
  assertion, or control flow never reached the assertion.
* Skips: a Suss skip the oracle did not make, or the reverse, is a recorded
  mismatch. A skip is never a pass.

Results must equal the reviewed known-failure baseline exactly:
`tests/oracle/clojure-test-suite-known-failures.json` lists each failed
namespace with its stage and exact diagnostic (which accounts for all of its
assertions), test failures, individual assertion failures and skip mismatches.
New failures, changed failures and unexpected passes all fail the test. After an
intentional change, record with `SUSS_CLOJURE_TEST_SUITE_WRITE=1` and review the
diff.

### Harness self-test

The real suite cannot load yet, so
[`tests/clojure-test-suite/fixture`](../../tests/clojure-test-suite/fixture) holds
two namespaces in the suite's shape, limited to what Suss executes today. They
run through the same oracle and harness with their own reference and baseline.
They cover predicates over scalars and collections, a negative-zero bits check,
value forms, `and`/`let` heads, `p/thrown?`, `ex-data`, `are` templates, ordered
effects, an opaque function operand and a load-time skip. Current result:
**21 pass (1 verdict-only), 0 fail, 1 matching skip.** The Python unit tests
cover the failure paths (wrong operands, undecodable operands, kind and verdict
mismatches, namespace and test failures, unexpected assertions and skips).

## Current suite result

At this commit the suite reports **0 pass, 0 fail, 5,834 not executed**: every
namespace fails before its tests run. The first blocker per namespace is:

| Namespaces | Blocker |
| --- | --- |
| 82 | `#?@` splicing reader conditionals |
| 72 | `:refer-macros` in `:require` |
| 57 | ratio or precision-suffix literals (`1/2`, `1N`, `1.0M`), including in unselected branches |
| 28 | reader dispatch: `#(...)` and `#"..."` |
| 9 | auto-resolved keywords (`::k`) |

Further gaps sit behind these: `clojure.*` to `cljs.*` namespace aliasing (the
suite requires `clojure.test`), mixed runtime/macro `.cljc` namespaces (the
portability helpers define macros beside functions), and much of `cljs.core`.

The harness also measured compiled-macro costs that block running the suite at
scale (reported on #14): `defn` expansion grows quadratically with preceding
definitions; every macro expansion materializes the complete `&env` analysis
graph, which grows with the enclosing form and namespace; unquote-splicing is
not expanded in ordinary functions of a macro namespace; and moderately sized
macro namespaces exceed the 65,536-node analysis graph bound. The harness avoids
`defn` and keeps syntax quote inside `defmacro` bodies for these reasons.

## Host interop and legacy overlap

[`clojure-test-suite-host-interop.json`](clojure-test-suite-host-interop.json)
classifies every `js/` reference in the vendored tests (26 references in 11
files) as `suss-branch` (a portable value or a `:suss` adaptation is possible),
`harness` or `host-specific`, with a rationale and alternative. The lock check
fails if a reference is unclassified or a classification is stale.

[`clojure-test-suite-legacy-overlap.json`](clojure-test-suite-legacy-overlap.json)
maps the 201 legacy cases to suite namespaces testing the functions they call:
171 have function-level suite coverage; 30 exercise literals and special forms
(`if`, `cond`, `do`, `let`, `fn`) that the suite does not test, so the legacy
corpus is retired only after those move to a precision corpus.

Inventory reviews may cite suite evidence in `:tests` as
`clojure-test-suite:<namespace>/<test>` or `clojure-test-suite:<assertion-id>`;
`scripts/cljs_reviews.py` rejects references that do not exist in the reference.

```sh
python3 scripts/clojure_test_suite.py                    # lock and host-interop checks
python3 scripts/clojure_test_suite.py overlap            # legacy overlap is current
python3 scripts/clojure_test_suite.py oracle [--suite fixture] [--write]
cargo test -p suss-compile --test clojure_test_suite --locked
```
