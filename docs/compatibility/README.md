# ClojureScript compatibility evidence

The [development oracle](../../tests/oracle/README.md) executes the pinned
ClojureScript source in Node and records lossless tagged reference observations.
Its 16-case corpus is shared with independently decoded Suss execution:
**9 differential passing, 7 failing, 0 skipped**. Exact tagged observations and
failure stages are tracked separately from the legacy baseline. Stable known
failures do not establish compatibility; ExceptionInfo implementation and broader
semantic/arity coverage remain M2/M4/M7 work. The bounded M1-02 evidence-harness
acceptance is complete; known failures are not compatibility successes.

The contract is [the design specification](../design/suss-0.3.1.md), using the
pinned ClojureScript submodule. `cljs-core.edn` contains **1,065 source declarations**
from core.cljs and core.cljc, with declaration kind, phase, source range, reader
context and SHA-256. This is a source inventory, not 1,065 implemented functions
or a count of public APIs. Both reader branches are retained; private definitions,
protocols and types are included because portable forms depend on them.

```sh
python3 scripts/cljs_inventory.py
python3 scripts/cljs_inventory.py --check
python3 scripts/cljs_reviews.py
python3 -m unittest discover -s scripts -p 'test_*.py'
```

The scanner does not execute source, descend into macro templates, or infer
portability from host interop text. Review module-level declarations generated
by macros, protocol methods, constructors and conditional branch applicability
before declaring the inventory complete as a public API contract.

`reviews.edn` is the manual overlay, keyed by generated declaration ID. Each
review must record visibility, arities, dependencies, classification
(`:portable`, `:adapted`, `:host-specific`), rationale, implementation status,
adaptation path and tests. Unreviewed entries remain `:unassessed`; they are not
excluded. Source hash changes invalidate the review. Ported source retains EPL
notices and must have reproducible extraction/patch provenance.

## Review overlay schema

`python3 scripts/cljs_reviews.py` validates the pinned source and the strict EDN
data overlay. It rejects stale source hashes, unknown/duplicate IDs, missing or
unknown fields, invalid arities and unsupported schema versions. It parses data
only. The initial empty overlay reports **0 reviewed, 1,065 unassessed**.
A review is not an executing test result; `:tests` lists evidence references for
reviewers to inspect. Full compatibility requires the separate executing suites.

Each `:reviews` entry is keyed by the exact generated declaration ID and contains:

| Field | Required value |
| --- | --- |
| `:source-sha256` | Current declaration hash from the generated inventory |
| `:visibility` | `:public`, `:private` or `:generated` |
| `:arities` | `{:fixed [0 1] :variadic-min nil}` with sorted unique nonnegative arities; nil for a noncallable declaration. defn/defmacro reviews require explicit arities |
| `:dependencies` | Vector of unique dependency names or declaration IDs, retaining phase qualifications where needed |
| `:classification` | `:portable`, `:adapted` or `:host-specific` |
| `:rationale` | Nonempty explanation based on the portable contract |
| `:status` | `:unimplemented`, `:in-progress`, `:implemented` or `:excluded` |
| `:adaptation-path` | Nonempty extraction/patch reference for `:adapted`; nil allowed otherwise |
| `:alternative` | Explicit replacement for excluded host-specific behavior; nil allowed otherwise |
| `:tests` | Vector of unique test/evidence references; at least one required for `:implemented` |

Excluded entries must be host-specific and name an alternative. The validator
does not infer portability, inspect dependency implementation, execute referenced
tests or certify extraction/patch provenance. Protocol methods and macro-generated
constructors remain explicit manual review work; the scanner's 1,065 top-level
declarations are not a public API completeness claim.

The [source and license policy](PROVENANCE.md) applies before importing core forms.

## Prototype baseline

The current legacy corpus has **201 passing cases, zero known failures and zero
skips** after the foundation repairs. This is a small, previously curated corpus;
it does not establish full upstream compatibility.

`cases.json` records every reviewed case ID, expression and expected value. Its
check catches removed passing cases and changed expectations, even when the
failure baseline is empty. This generated data retains the source test suite's
[attribution and EPL notice](../../reference/cljs-tests/README.md).

`known-failures.json` records each currently failing legacy case, its failure
stage and exact diagnostic/decoded output. Ordinary conformance tests fail on
new failures, changed failures, removed cases and unexpected passes. Passing
this baseline test means **no change to reviewed failures**, not full compatibility.

The independent Rust decoder reads the prototype GC heap. It supports scalar
values, keywords/symbols, persistent vectors (including trie nodes), maps, sets,
Cons, IndexedSeq and MapEntry. Unsupported values, including unrealized LazySeq,
are decode errors. It does not invoke the Suss printer or equality to decide
whether a test passes. Decoder layout knowledge is intentionally isolated in
`tests/support/decode.rs` and must change with the runtime ABI.

When intentionally adding/changing test inputs, regenerate `cases.json` with
`cargo test -p suss-compile --test conformance record_case_catalog -- --ignored --exact`
and review removed cases and changed expectations.

To capture observations after intentionally changing semantics:

```sh
cargo test -p suss-compile --test conformance record_known_failures -- --ignored --nocapture
```

Review every diff. Do not accept new failures merely to get a green build. Fix
regressions and remove resolved entries. The shared development differential
oracle now records lossless UTF-16/float transport and effect traces separately from this legacy baseline. Comprehensive
portable semantic/arity coverage and repairs for its exact known failures remain
M2/M4/M7 work.

The conformance loader rejects missing files, empty/malformed suites, duplicate
or unknown fields/IDs, namespaced schema keys and non-Boolean/true skip values.
Baseline/catalog JSON maps reject duplicate keys and trailing data. Executing
negative GC fixtures verify unknown tags, malformed boxes and non-string arrays
fail decoding. Harness regressions also check new/changed failures and unexpected
passes against exact stage/diagnostic records.

## Portable reader foundation

The separate [reader-form boundary](../runtime/reader-forms.md) preserves source
spans, metadata, binary64 and UTF-16. Its 14 scalar observations match fresh
pinned ClojureScript reader execution and are transferred through ABI runtime
intrinsics. They do not change the compiler corpus's 9 passing/7 failing/0 skipped
counts; reader/IR/backend and namespace-phase integration remain incomplete.

## Portable compiler bootstrap evidence

The [new HIR/IR path](../runtime/portable-pipeline.md) compiles the 14 scalar reader
cases into actual ABI fragments and executes a separate 34-case source corpus
whose observations match fresh pinned ClojureScript/Node exactly. It remains a
bounded bootstrap; legacy CLI/AOT/macros and the 16-case full source corpus have
not migrated. The latter remains 9 passing/7 exact failures/0 skips. No inventory
entry is marked implemented by these counts; all 1,065 remain unassessed.
