# Reachable scratch for recorded metadata

This port adapts the preserved f56f1ab native index and occurrence planner to
base `ad6bbcc3c1687f717c6638b127673a0bc83baac8`. Recorded metadata subroots use
invocation-local scratch indexed by reachable native recipe IDs when the
enclosing graph exceeds4096 recipes. Smaller recorded graphs, ordinary
materialization and controlled whole-graph materialization remain dense.
The threshold selects an implementation and rejects no form.

Both modes share traversal, callbacks, construction and private-placement logic.
New map lookups, initialization, allocation and growth are charged before work.
The index contains native IDs, not guest references or cached source authority.
All reachable data and existing integrity checks remain; no callback or failed
operation is caught and replayed.

## Executed scopes

The last V8 cohort executes all33 selected tests: **30PASS,3fuelFAIL,0ignored**.
It includes the original32-test preservation cohort. Separately, production-
identical code passes14 guards in V3,4 in V4 and1 in V6. The full selection
therefore covers52 unique tests with49 positive outcomes and3 remaining fuel
failures across explicitly separate source tuples. It is not a fresh52-test run
on one final head. Earlier failed new-test assumptions and unexecuted tails are
retained in the archive.

The original invocation-tag regression and pinned-map wrapper pass. Remaining
failures are inference during caller `&env` transport at382..414, complete core
environment during caller `&env` transport at0..33, and collection source during
invocation at851..882. Existing budgets and fixtures are unchanged.

The independent map comparison verifies all69 ordered assertion IDs:62 host
passes,7 guest judgments, no failures, skips or unexecuted assertions. Guest
judgments are not independent host passes.

Source comparisons retain both caller phases, map widths2/128, padding0/20000,
owner/revision identity, GC and nested-payload poison. Wide Runtime metadata
remains logically correct but lacks memo receipts for direct certification;
Macro authenticates those facts. Both modes enforce that existing policy and
reject logical payload poison. The new cost-comparison fixtures explicitly
use200M for diagnostics; no production allowance or original test budget changes.
Sparse indexing can cost more for wide reached roots; adverse cases are retained.

The fuel test retains lower typed-exhaustion samples. At the last-unit sample,
Wasmtime can return from its final block with zero reported fuel before the
next check. All four phase/mode cases verify zero remaining fuel, then a typed
OutOfFuel trap on the next positive host debit in the same scope. Retained value
contents after GC, source immutability, cancellation and fresh recovery also
pass. Cold and warm costs are equal here; the earlier cache explanation was
disproved. Exception tests inspect actual language values and capture a fresh
bridge for the explicit recovery request after restoring its hash binding.

Normal bootstrap generation and identity verification pass. A final exact
three-line comment correction regenerates only BOOT4; both Wasm modules retain
byte-identical noncustom sections. The archive distinguishes that final
publication tuple from the executed V8 source and credits no new semantic run
to the comment refresh. All executed input freezes, commands, environment
overrides and raw outcomes remain source-bound.

## Separate measurements and remaining gates

The included preceding ad6 diagnostic completes the first Runtime expansion at
1,706,700,367fuel units. Its environment interval consumes1,706,480,543units;
825 materializations use a100,545-recipe graph. The824 plain subroots reach
4–117 nodes each, median11. Selected fixed scratch debits total414,848,670units,
24.310% of that interval. This is not a predicted net saving or a measurement
of this port's complete compilation cost. The following expansion still
exhausts its diagnostic2B allowance; whole-program required cost is unknown.

Preceding [ad6 CI39166019279](https://github.com/bobby/suss/actions/runs/39166019279)
is red: foundation passes; integration127PASS/17fuelFAIL,32/252 targets completed
and220 unexecuted. It confirms the restored invocation-tag test but does not
validate this new port.

`cargo test --workspace --locked -- --test-threads=2`, independent final-head
review and passing final-head CI remain acceptance gates. The full local
baseline was not repeated while original focused acceptance remains red.
No issue closure, draft promotion or merge is claimed.

## Archive

[Source, execution, controls and publication evidence](m4-pr256-recorded-metadata-scratch-2026-10-18.tar.gz),
SHA256 `3def04d1eb59916d9f209602224c96611d50662a116d5882fc0a9bccf0d1eb32`.
It includes234 hashed payloads representing594 logical files, the preceding
diagnostic bundle and CI logs. Final Git correspondence is checked separately
against the nine publication source/bootstrap hashes after committing.
