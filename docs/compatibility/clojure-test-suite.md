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
only, requires every recorded assertion to pass, and fails unless the result
equals the reviewed
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

Recorded forms canonicalize process-global auto-gensym counters (`p1__N#`), which
otherwise differ between compiler runs. Values use the transport tags of
`suss-oracle.main` (binary64 bits, UTF-16 code
units, collection kinds). Observation never changes the program: a sequence with
an unrealized tail is recorded as `opaque` instead of being forced, and values
the transport cannot represent (functions, atoms, UUIDs, JavaScript objects) or
that exceed a 20,000-node budget are `opaque`. `Math.random` is replaced by a
seeded generator and the suite runs under four seeds. Assertions whose
observations differ are rejected unless they are in the reviewed
`RANDOMIZED` list (random-number tests); those keep only kind and verdict.
Asynchronous tests (`taps`) complete before output is written.

At `95d4a91` the oracle records 231 tests and **5,834 assertions, 0 failures,
0 errors**; 52 are randomized (verdict only), 102 operands or values are opaque
and 134 contain an opaque node. The 20 oracle skips are the suite's
`when-var-exists` skips of vars ClojureScript lacks: `bigdec`, `bigint`,
`bound-fn`, `bound-fn*`, `decimal?`, `denominator`, `format`, `intern`, `num`,
`numerator`, `+'`, `*'`, `ratio?`, `rational?`, `rationalize` and
`with-precision` at load time, and inside tests `future` twice and `promise`
once (`realized?`) plus `promise` once (`ifn?`). All are JVM-only APIs outside
the portable contract.

## Suss harness

`scripts/test-clojure-test-suite.sh` (the issue's acceptance command) runs the
offline checks and `cargo test -p suss-compile --test clojure_test_suite`. The
harness builds its source tree with `clojure_test_suite.py suss-sources`: the
vendored tests, the Suss-only patch `0002` (`:suss` branches for the JavaScript
Number range constants) and the harness namespaces in
[`tests/clojure-test-suite/suss`](../../tests/clojure-test-suite/suss). It loads
every namespace, in the oracle's order, through the production `Session` and
`CompiledMacros` with the shipped compiled core, then runs each namespace's tests
as one group under its `:once`/`:each` fixtures in the oracle's test order.

The suite requires `clojure.test` and its portability helpers, which define
macros beside functions and are referred with `:refer-macros` or `:refer`. A Suss
runtime namespace cannot yet define macros, and Suss lacks `clojure.*`/`cljs.*`
aliasing and `:refer-macros`. The harness therefore rewrites only each suite
file's `ns` form, after reading it with the Suss reader: the `clojure.test` and
portability libspecs become runtime requires of `suss.harness.test` and the Suss
`clojure.core-test.portability`, plus `:require-macros` of the harness macros for
names that are macros upstream. Every other clause and all other source is
unchanged; files Suss cannot read are not rewritten, and their read error is
recorded. The Suss `when-var-exists` always expands its body: a var Suss lacks
fails its namespace with an exact diagnostic, and the oracle's skip becomes a
recorded mismatch. Suss never reports a skip it did not prove.

The harness `is` evaluates predicate operands once, left to right, then applies
the predicate, evaluates the message, returns the result as `cljs.test` does and
records the raw values with the assertion's head symbol. The host decodes them
with the independent test decoder (`tests/support/portable_decode.rs`). Records
are decoded in bulk and, when that fails, element by element, so one undecodable
operand is opaque on its own. Guest equality never decides a result.

`scripts/clojure_test_suite.py compare` decides each oracle assertion:

* Pass: same kind, same head symbol and a passing verdict, and every deterministic oracle operand
  matches the Suss operand with `oracle_compare.matches` (exact binary64 bits
  apart from the canonical NaN sign, distinct collection kinds, unordered maps and
  sets). A predicate the oracle reports by value is compared by its result.
* Verdict only (`guest-judged`, counted separately from `pass`): randomized
  assertions and opaque oracle operands. An opaque Suss operand where the oracle
  has a value is a failure.
* Fail: wrong kind, head, verdict or observation; unexpected assertions; every
  assertion of a test whose assertion count differs from the oracle's (ordinals
  no longer align, so none may pass by coincidence); each skip mismatch.
* Not executed: the namespace failed to load, the test failed outside an
  assertion before reaching it, or control flow never reached it.
* Skips are compared as multisets: a Suss skip the oracle did not make, or the
  reverse, is a failure. A skip is never a pass.

Namespace failures record a stage: `read` when the Suss reader reproduces the
diagnostic, `macro` for compiled macro failures, and `namespace` otherwise.

The test decoder cannot yet decode lazy sequences (even realized ones), ranges,
records, functions or atoms. Once more namespaces load, correct Suss results of
those types will appear as `observation:undecodable` failures until the decoder
learns their layouts; they never pass by default.

Results must equal the reviewed known-failure baseline exactly:
`tests/oracle/clojure-test-suite-known-failures.json` lists each failed
namespace with its stage and exact diagnostic (which accounts for all of its
assertions), test failures, individual assertion failures and skip mismatches.
New failures, changed failures and unexpected passes all fail the test. After an
intentional change, record with `SUSS_CLOJURE_TEST_SUITE_WRITE=1` and review the
diff.

### Harness self-test

[`tests/clojure-test-suite/fixture`](../../tests/clojure-test-suite/fixture) holds
three namespaces written exactly in the suite's `ns` shape, limited to what Suss
executes today, with their own oracle reference and baseline. They cover
predicates over scalars and collections, a negative-zero bits check, value forms,
`and`/`let` heads, `p/thrown?`, `ex-data`, `are` templates, ordered effects, an
opaque function operand, a `:once` fixture and a `when-var-exists` skip of a
missing var. Current result: **22 of 23 assertions pass (21 host-decided, 1
verdict-only)**; the skip namespace fails to load as designed, so its one
assertion is not executed and the oracle's skip is a recorded mismatch. The
Python unit tests cover the remaining failure paths.

## Current suite result

At this commit 12 suite namespaces load and run (`bit-clear`, `bit-flip`,
`bit-not`, `bit-shift-left`, `bit-shift-right`, `bit-test`, `disj!`, `dissoc!`,
`hash-set`, `identical?`, `name`, `namespace`). Their **99 assertions all pass**
host-side against the oracle. The other 236 namespaces fail before their tests
run, so 5,735 assertions are not executed; the 20 failures are the oracle's 20
skips, which Suss does not make. Namespace failures by stage:

| Namespaces | Stage | Blocker |
| --- | --- | --- |
| 86 | read | `#?@` splicing reader conditionals |
| 58 | read | ratio or precision-suffix literals (`1/2`, `1N`, `1.0M`), including in unselected branches |
| 28 | read | reader dispatch: `#(...)` and `#"..."` |
| 9 | read | auto-resolved keywords (`::k`) |
| 51 | namespace | missing `cljs.core` vars or macros (`range`, `str`, `take`, `constantly`, `volatile!`, `letfn`, `when-let`, printing, ...) and `clojure.string`/`clojure.core` namespaces |
| 4 | macro | compiled macro data limits and invalid macro data |

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
fails if a reference is unclassified or a classification is stale. The
`number_range` constants have `:suss` branches (patch `0002`); the harness
replaces the portability `sleep` helper. Host-specific assertions remain in the
counts as failures or not executed; the classification explains them.

[`clojure-test-suite-legacy-overlap.json`](clojure-test-suite-legacy-overlap.json)
maps the 201 legacy cases to suite namespaces testing the functions they call:
171 call at least one function with a suite namespace; 30 exercise only
literals and special forms (`if`, `cond`, `do`, `let`, `fn`) that the suite does
not test, and 40 of the 171 also depend on `if`/`loop`/`let`/`recur`. Retiring
the legacy corpus therefore requires moving its special-form and literal cases to
a precision corpus, not only suite coverage of its functions.

Inventory reviews may cite suite evidence in `:tests` as
`clojure-test-suite:<namespace>/<test>` or `clojure-test-suite:<assertion-id>`;
`scripts/cljs_reviews.py` rejects references that do not exist in the reference.

```sh
scripts/test-clojure-test-suite.sh                       # acceptance command
python3 scripts/clojure_test_suite.py                    # lock and host-interop checks
python3 scripts/clojure_test_suite.py overlap            # legacy overlap is current
python3 scripts/clojure_test_suite.py oracle [--suite fixture] [--write]
cargo test -p suss-compile --test clojure_test_suite --locked
```
