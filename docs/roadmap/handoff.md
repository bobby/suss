# Implementation handoff — 2026-09-29

## Start here

The requested design and roadmap are now repository artifacts:
[specification](../design/suss-0.3.1.md), [roadmap](../../ROADMAP.md),
[39 work packages](issues.json), [upstream inventory](../compatibility/README.md).
All 10 milestones and 39 issues were published in
[bobby/suss](https://github.com/bobby/suss/milestones); IDs/URLs are in
[github.json](github.json). The publisher is idempotent and preserves existing
issue bodies. Its default operation is an offline preview.

The language resurrection itself is **not complete**. The current work establishes
an executable, measurable prototype baseline and feasibility evidence. WASI 0.3.1,
persistent compiled REPL, ABI v1, upstream core import and browser delivery remain
open. Do not claim those features based on these repairs or narrow CLI probes.

## Changes made

* Replaced stale README/ROADMAP/agent guidance with the accepted contract. Marked
  METADATA_DESIGN as historical. Left prototype source in place pending replacement.
* Added a deterministic source scanner and 1,065 hashed upstream declarations.
  Five scanner tests cover metadata, comments, strings/regex/chars, conditional
  branches, quoted templates/discards, prime symbols and malformed delimiters.
  Classification remains explicitly unassessed; no core forms were imported.
* Replaced permissive conformance reporting with independent Rust GC decoding,
  strict case parsing, exact outcomes, a reviewed input/expectation catalog and
  a known-failure file. The
  curated 201-case corpus now passes with zero known failures or skips. This is
  not a full ClojureScript oracle or an upstream coverage percentage.
* Fixed conditions evaluating effects three times; normalized comparison operands
  before branching; made integer arithmetic/comparisons evaluate left to right.
  Equality arguments are no longer lowered as tail calls that return collections
  instead of booleans. Protocol local declarations now include anonymous temps.
* Added two-argument `reduce`, including empty/singleton collections, using a
  temporary variadic adapter. Full Reduced/protocol reduction remains M4-04.
  Added static diagnostics for known fixed/minimum arities. Lexical callbacks now
  shadow namespace functions and intrinsic names at call sites.
* Enabled final component validation. Repaired WIT parameter offsets, missing
  float casts, and calls that bypassed canonical return conversion. Preserved
  direct export tail calls; 10,000-step self-recursion still works. Added executing
  component tests for internal calls, floats, options, list returns, closures and protocol dispatch.
* Added a three-module shared-GC feasibility test: closure calls and objects
  survive forced GC; descriptor identity distinguishes same-layout types; ABI
  mismatch is rejected by the prototype loader check. This is a fixture, not
  production REPL/runtime implementation.
* Added Cargo.lock, lean build/test profiles, shared test engines, bounded fuel
  and decoder traversal, and a GitHub Actions baseline workflow. Remote CI has
  not run because these repository changes have not been pushed.
* Added a real Chrome feasibility fixture for ES modules, GC continuation
  suspension/resume, cancellation and stale callback isolation. The browser
  emitted passing DOM but needed termination during shutdown; this is recorded
  explicitly in browser-probe.json. It does not compile Suss source.
* Added eight candidate CLI probes; see [toolchain evidence](toolchain.md).
  Production Rust dependencies are still Wasmtime 39.0.1 and wasm-tools 0.221.3.

## Validation

Routine compiler warnings predate this work; no blanket warning suppression was
added. Twelve pre-existing debug-dump tests and two existing documentation examples
remain ignored. The two new baseline/catalog recording commands are deliberately
ignored during ordinary tests because they write evidence.

* `python3 -m unittest discover -s scripts -p 'test_*.py'`: 5 passed.
* `python3 scripts/cljs_inventory.py --check`: 1,065 declarations verified.
* `python3 scripts/publish_roadmap.py`: offline validation of 10 milestones/39 issues.
* `python3 scripts/probe_toolchain.py --wasmtime <49.0.1> --wasm-tools <1.258.0>`:
  8 probes passed; four execute core Wasm, three compile component declarations,
  one parses/prints WIT. See JSON for source hashes and precise limits.
* Initial full workspace testing exposed lexical-shadowing and exported TCO
  regressions from stricter checks; both were repaired and rerun specifically.
* `cargo test --workspace --locked -j 2 -- --test-threads=2 --skip conformance_matches_reviewed_baseline`:
  passed (14 CLI, 54 compiler unit, 316 expression integration, 27 component,
  2 conformance helper, 1 shared-runtime, 8 core and 19 reader tests). The slow
  conformance corpus was run separately rather than twice.
* `cargo test -p suss-compile --test conformance --locked -- --test-threads=2`:
  strict corpus and decoder/comparison tests passed: 201 cases, zero failures/skips.
* Final targeted changes after that workspace run were checked separately:
  `cargo test -p suss-compile --test component --locked -- --test-threads=2`
  passed all **28** tests, including the added closure/protocol export case;
  `case_catalog_matches_reviewed_inputs` passed for all 201 expressions/expectations.
* `cargo test -p suss-compile --test compile_expr test_tco_ -- --test-threads=2`:
  4 passed, including 10,000-step exported recursion. Compiler unit (54), core (8)
  and reader (19) suites also passed their focused reruns.
* Chrome feasibility: five semantic checks passed; process shutdown required
  termination after completed DOM (see toolchain.md).
* `git diff --check`: passed.

## Next implementation package

Continue **M0-02** before a production dependency/runtime replacement:

1. Lock the official WASI 0.3.1 package graph with source versions and hashes.
   Preserve each package's own published version; do not rewrite all versions
   to 0.3.1. Keep the old bundled WIT only until its replacement passes tests.
2. Migrate Rust dependencies to the candidate compatible family (Wasmtime 49.0.1,
   wasm-tools libraries 0.258.0, matching wit-bindgen). Re-run shared_runtime and
   component fixtures using the new engine. CLI acceptance is not Rust API proof.
3. Add actual bidirectional map values and canonical async/callback execution,
   including cancellation and future/stream transfers. Type declarations alone
   do not satisfy this gate. Record unsupported features explicitly.
4. Extend the M0-04 Chrome fixture to the full feature profile and compare the
   optional Jco path before committing browser packaging. Then implement
   spans/namespace phases and the general verified IR (M2).

## Known limitations and implementation cautions

* The prototype emits a private print import even for pure libraries. Executing
  fixtures explicitly supply it; removal is M5-01, not already solved.
* WIT option currently collapses none to nil; exact tagged boundary representations,
  all WIT types/resources, realloc/free/post-return and async lifetimes are future
  work. Existing tests do not certify allocation safety or full interoperability.
* Export-to-export conversions still use the prototype's limited calling rules.
  The target design separates language calls from generated canonical adapters.
* Numbers/strings remain prototype i31/i64/f64 and UTF-8, not the intended ordinary
  f64/UTF-16 contract. Nominal runtime tags still need ABI v1.
* The decoder rejects unsupported objects (including unrealized LazySeq). Never
  restore an opaque-object wildcard to make such a test pass.
* The reader inventory is lexical, retains both reader branches and does not
  evaluate generated declarations. Review protocol methods, constructors,
  generated definitions, visibility, arities and dependencies before claiming
  complete portable API coverage.
* REPL still replays source; macros still use the temporary evaluator. Multi-arity
  definitions are handled inconsistently by the two analysis paths. The reduce
  adapter is temporary and documented at its source.
* Test compilation still emits the entire prototype core per expression. Engines
  are reused, but a full suite takes minutes; replacing this is part of the shared
  runtime work. Use two test workers locally and retain bounded CI timeouts.
* Existing untracked `.codex/`, `reference/BUSINESS_DSL_RESEARCH.md` and
  `reference/clojure-site/` were left untouched. AGENTS.md was already untracked
  and was intentionally rewritten. No Git commit or push was made.
