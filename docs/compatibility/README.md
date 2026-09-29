# ClojureScript compatibility evidence

The contract is [the design specification](../design/suss-0.3.1.md), using the
pinned ClojureScript submodule. `cljs-core.edn` contains **1,065 source declarations**
from core.cljs and core.cljc, with declaration kind, phase, source range, reader
context and SHA-256. This is a source inventory, not 1,065 implemented functions
or a count of public APIs. Both reader branches are retained; private definitions,
protocols and types are included because portable forms depend on them.

```sh
python3 scripts/cljs_inventory.py
python3 scripts/cljs_inventory.py --check
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
regressions and remove resolved entries. The broader upstream differential oracle,
lossless UTF-16/float transport and effect traces are still roadmap work.
