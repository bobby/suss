# Portable when-let source and observations

The implementation retains the complete pinned ClojureScript macro at
`c4295f303100bbf5afac449242d30bca1126f1a1`, including the docstring,
vector invocation, syntax quote, initializer temporary and inner binding form.
`when-let.patch` records the local declaration spelling and ordered compiled
`assert-args` expansion. EPL-1.0 notices and the license are retained here and
in the Rust source. `provenance.json` records exact source spans and hashes.

Bootstrap `let` previously rewrote only its head to `let*`, whose binding names
must be symbols. The prerequisite reuses the existing function-parameter
destructuring algorithm for vector/map patterns in ordered binding pairs.
Its extraction calls remain ordinary live source calls. It introduces no
function scope or recur target; ordinary symbol bindings and loop lowering
retain their existing path. Complete pinned `let` and `destructure` forms are
retained for comparison. This is compiler syntax lowering, not a native
when-let evaluator or certification of generated syntax as original source.

`oracle/` retains the exact development inputs, commands, raw logs, receipts,
preparation failures and license. Two fresh pinned CLJS compiler/Node runs
agree on the 16 tagged observations copied from the original native fixture.
Two fresh self-host macro-function runs agree on five invalid binding
diagnostics, phase data and a valid subsequent expansion. The diagnostic
oracle invokes the actual compiled pinned macro function; it does not promise
the same analyzer wrapper diagnostics. The gensym spelling in the retained
recovery expansion is incidental. JVM and Node are development tools only.

The direct-let regressions additionally cover ordered initializers,
eager map defaults, earlier/outer local scope, generated-name hygiene and
tail recur. `direct-let-oracle/` retains both exact native guest substrings,
two fresh pinned compiler/Node runs, raw commands/logs/receipts, explicit tagged
expected values and a hash manifest. Both runs match both native expectations:
`[17 19 [17 19] 23 19 59 77 [1 4 2 3 5]]` and `[[11 7 18] 6]`.
Both native regressions also passed in each default caller phase. The checker
binds both native fixture hashes and compares their current guest substrings
against the retained oracle inputs; source changes invalidate the evidence.

The initial before capture contains the unchanged native test SHA
`f85bc8c1591ac6fb51274ec8230deb0880e3c1f4e387ddacf9b66f1fca3e2e02`.
Its three when-let tests failed on unresolved names; three decoder support
tests passed. The Jank before run executed zero assertions with all 12 IDs
unexecuted because the namespace could not resolve when-let. The gate was
subsequently strengthened with explicit reference schema, suite and CLJS-pin
assertions; its complete original body and 12-ID selection remain unchanged.
The before gate hash is retained in provenance and the immutable before snapshot.

The native focused tests use both default caller phases. The Jank harness
uses Runtime and its existing 200-million allowance; it is a separate gate.
It rejects incomplete IDs, guest assertions, skips, test/namespace errors and
late failures. The focused post-pipeline passed 17 Rust executions: private
validation 1, native feature tests 5 plus decoder support tests 6, Jank wrapper 1,
and bootstrap verifier 4. Separately, all 12 Jank assertions were host-decided
passes in exact complete order, with no missing IDs, errors or skips.
Bootstrap generation succeeded; verification reproduced both phase artifacts
byte-for-byte. Across the pipeline, 1825 frozen inputs remained unchanged
except the four permitted regenerated bootstrap files. The full required
workspace baseline and final-head CI remain pending.

Read-only provenance check:

```sh
PYTHONDONTWRITEBYTECODE=1 python3 scripts/check_when_let_provenance.py
```

Actual focused pipeline commands (capture directory shown explicitly):

```sh
cargo run --profile test -p suss-cli --bin suss-bootstrap --locked -- runtime/bootstrap
cargo test -p suss-compile --locked --lib portable_macros::when_let_validation_tests:: -- --test-threads=2 --nocapture
cargo test -p suss-cli --locked --test compiled_when_let --test compiled_let_destructuring -- --test-threads=2 --nocapture
SUSS_WHEN_LET_CAPTURE=/home/bobby/.cache/suss-m4-work/when-let-after cargo test -p suss-compile --locked --test clojure_test_suite pinned_when_let_complete_body_matches_reference -- --exact --test-threads=2 --nocapture
sh scripts/verify-bootstrap.sh
```

The pipeline also passed `python3 scripts/cljs_reviews.py`. Raw stage commands,
outputs, source freezes and terminal receipt are in the retained
`when-let-after` capture. No broader namespace-sharing, source-integrity or M4
acceptance follows from these focused results. The required full command is
`cargo test --workspace --locked -- --test-threads=2`.
