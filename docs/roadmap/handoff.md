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
  and decoder traversal, and a GitHub Actions baseline workflow. The first published roadmap-branch CI is now running; see the latest
  publication evidence below. No remote success is claimed yet.
* Added a real Chrome feasibility fixture for ES modules, GC continuation
  suspension/resume, cancellation and stale callback isolation. The browser
  emitted passing DOM but needed termination during shutdown; this is recorded
  explicitly in browser-probe.json. It does not compile Suss source.
* Added eight candidate CLI probes; see [toolchain evidence](toolchain.md).
  Discovery used Wasmtime 39.0.1 and wasm-tools 0.221.3; the current
  M0-02 migration below replaces that family.

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

## M0-02 source-lock increment — 2026-09-29

Discovery changes were committed as `6306fd6`. The unrelated pre-existing
`reference/BUSINESS_DSL_RESEARCH.md` and `reference/clojure-site/` remain untouched.

Vendored the byte-exact official WASI v0.3.1 WIT release archive under
`vendor/wasi/wasi-wit-0.3.1/`, with upstream license and source provenance.
`wasi-wit-lock.json` pins release commit, archive hash, all 15 file hashes,
six original package identities and their local dependency graphs. The optional
`--archive` check also verifies the downloaded archive hash and exact member
contents against the tracked source lock. Dependency
copies may contain subsets of the primary package; they are preserved exactly.
A version-preservation regression uses a 0.2.7 dependency in a 0.3.1 package.

* Regression-first: `test_wasi_lock.py` initially failed because the verifier
  did not exist. Tests now reject missing/changed/extra files, missing local
  dependencies and altered lock graphs, while accepting exact dependency subsets.
* `python3 -m unittest discover -s scripts -p 'test_*.py'`: 12 passed.
* `python3 scripts/wasi_lock.py`: 15 files/six packages verified offline.
* `python3 scripts/wasi_lock.py --wasm-tools /tmp/suss-toolchain-49/wasm-tools-1.258.0-aarch64-macos/wasm-tools --output docs/roadmap/wasi-wit-probes.json`: all six graphs resolved and validated binary WIT round-trips passed.
* CI now checks the source lock offline, alongside inventory checks.

This proves source integrity, dependency resolution and package encoding only.
It does not prove actual WASI imports, canonical async execution, maps crossing
a boundary or Suss binding generation. M0-02 and milestone M0 remain open.
The prototype bundled WIT and production Rust versions are unchanged.
Progress was posted to [M0-02](https://github.com/bobby/suss/issues/2#issuecomment-5896703530);
the issue retains milestone M0 and remains open.

## Next implementation package

Continue **M0-02** before a production dependency/runtime replacement:

1. The official WIT source graph is now locked (see above). Keep the old
   bundled WIT until the replacement bindings pass executing tests.
2. The Rust family migration now passes the full baseline (see below).
   Continue canonical callback, cancellation and future/stream feasibility.
3. Named M0-02 local feasibility probes now pass, including future/stream guest
   payload reads, one-byte demand/EOF and read cancellation (see the audit below).
   Publication and remote issue acceptance updates await explicit approval.
   Generated Suss bindings, nested payloads and lifecycle stress remain M5/M6.
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
  and was intentionally rewritten during discovery. Discovery was subsequently
  committed as `6306fd6` and subsequently published on the roadmap branch.

Current increment runtime check:
`cargo test -p suss-compile --test shared_runtime --locked -- --test-threads=2`
passed (one executing shared-GC fixture) on the existing Wasmtime 39.0.1 engine.

## M0-02 Rust toolchain migration — 2026-09-29

The discovery baseline process finished with exit 0. Its exact command was
`cargo test --workspace --locked -- --test-threads=2`: 14 CLI, 54 compiler unit,
316 expression integration (12 existing ignores), 28 component, four conformance
(two manual recorders ignored), one shared-runtime, eight core and 19 reader
passed. The strict conformance test covers 201 reviewed cases. Two existing
documentation examples remain ignored. Log: `/tmp/suss-discovery-baseline.log`.

The current worktree migrates exact direct versions to Wasmtime/wasmtime-wasi
49.0.1, wasm-encoder/parser and wit-parser/component 0.258.0, wit-bindgen 0.61.1.
The transitive WAT parser is also locked to 1.258.0. Only selected toolchain
packages were updated; unrelated lock entries were retained where compatible.
Rust 1.98.0 is the tested compiler (Wasmtime 49 requires at least Rust 1.96).

Adapted the parser's parameter/result/world item API, encoder float wrappers,
mutable ComponentEncoder API and checked u64 section offsets. Forced GC errors
now fail the shared-runtime fixture instead of being discarded. The prototype
WIT analyzer rejects unsupported shapes (including nested shapes), async functions
and external-id mappings before lowering instead of silently producing Unknown.
Supported type aliases resolve recursively. This is not a new WIT adapter system.

Regression-first evidence:
* Both Rust WIT profile tests failed with 0.221.3: map syntax and the official
  CLI async signature were rejected. They now pass with the pinned parser.
* Before the adapter diagnostics, map/async tests failed at component encoding
  instead of reporting the unsupported boundary. They now report Unsupported.
* The first implements execution fixture attempted to reexport an imported
  function; the engine rejected that unsupported route. The final fixture
  actually lowers the host call, invokes it from a core guest and lifts the
  guest export, proving the named implements instance is used.

Focused checks:
* `cargo check -p suss-compile --locked -j 2`: passed.
* `cargo check -p suss-cli --locked -j 2`: passed (native CLI).
* `cargo test -p suss-compile --test component --test shared_runtime --test toolchain_profile --locked -j 2 -- --test-threads=1`: 28 component, one shared-GC, five initial profile tests passed. One test worker was used while the old baseline was executing conformance.
* Final `cargo test -p suss-compile --test toolchain_profile --locked -j 2 -- --test-threads=1`: seven passed. This executes GC, typed function references, tail calls and exceptions; actual canonical map transfers with empty/Unicode/boundary values; a named implements import through a guest call; official WIT resolution; and explicit prototype unsupported diagnostics.
* `cargo check -p suss-cli --target wasm32-wasip2 --features component --locked -j 2`: **failed**, 20 errors. Native-only rustyline/Wasmtime imports and completer code are compiled on the Wasm target, and main references an unavailable run_component function. This optional legacy compiler-component target is not certified by the native checks. wit-bindgen 0.61.1 itself compiled, but the full CLI artifact did not. Log: `/tmp/suss-wit-bindgen-check.log`.

The migrated full baseline completed with exit 0:
`cargo test --workspace --locked -- --test-threads=2`. All 14 CLI, 54 compiler
unit, 316 expression integrations (12 existing ignores), 28 component, four
conformance (two manual recorders ignored), one shared-runtime, seven profile,
eight core and 19 reader tests passed. The strict corpus covers all 201 reviewed
cases. Two existing documentation examples remain ignored.
Log: `/tmp/suss-migrated-baseline.log`. No new skips or known failures were added.

Progress is recorded on [M0-02](https://github.com/bobby/suss/issues/2#issuecomment-5897039869),
which retains milestone M0 and remains open. Canonical callback suspension/resume/cancellation and actual
future/stream transfers are the next unblocked feasibility work. Maps have only
been exercised through a hand-written engine fixture; Suss map adapters are
still unsupported. external-id has parser evidence but no execution fixture.
The old bundled WIT remains active until generated replacement bindings pass
executing tests. The persistent compiled REPL and language ABI have not changed.

## M0-02 stackless callback and endpoint feasibility — 2026-09-29

The Rust migration was committed as `8156c97` after the complete migrated
baseline passed. Three new `toolchain_async` tests execute separately:

* A stackless canonical async lift enters, returns YIELD with the task pending,
  then receives a callback and invokes `task.return` once. Rust observes value
  42 and exact ordered trace `[1, 2, 3]`. Stackful async is explicitly disabled.
* A typed `future<u32>` read endpoint is lowered through a core guest and lifted
  back as a future, rather than implicitly awaiting it. A separate Rust consumer
  receives its payload 42 and consumes the endpoint with `pipe`.
* A typed `stream<u8>` endpoint similarly crosses the guest in both directions;
  a separate Rust consumer receives bytes `[0, 255, 42]` and closes after them.
  This bounded transport fixture does not certify EOF or backpressure semantics.

Regression-first: the callback test failed with a missing artifact before its
fixture existed; the future/stream tests likewise failed before their transfer
fixture was added. Final command:
`cargo test -p suss-compile --test toolchain_async --locked -j 2 -- --test-threads=2`
passed all three, with no ignored tests. Log: `/tmp/suss-endpoint-final.log`.
The bounded local future driver has a five-second deadline; the callback guest
also has Wasmtime fuel. No production compiler/runtime code changed after the
successful full migration baseline, so these tests were checked directly.

`callback-resume.wat` is adapted from the upstream Wasmtime callback WAST fixture
at `46c23a87dac1465986a8ad53ba6a7ae49372857b`. Source URL/hash, adaptation and
fixture hash are in `callback-provenance.json`; the upstream Apache-2.0 WITH
LLVM-exception license is retained beside the fixture. The endpoint WAT is an
original hand-written Suss feasibility fixture. Neither is compiled Suss async
nor the production scheduler or full WASI binding graph.

Pinned Wasmtime public API limitation: `call_concurrent` documentation says that
dropping its Rust future does **not** cancel a started guest task; individual
host-driven cancellation is unavailable. Upstream
[issue #11833](https://github.com/bytecodealliance/wasmtime/issues/11833) was
verified open. This is source/API evidence, not a passing cancellation execution
test. Do not substitute store destruction for the specified session/task API.
Guest-driven `subtask.cancel`/task cancellation remains the next unblocked probe,
along with canonical async imports and executing external-id semantics.
M0-02 and milestone M0 stay open. Suss async lowering and Suss boundary adapters
remain unsupported; M6 lifetime/cleanup and stress gates remain future work.

## Published branch and live CI — 2026-09-29

The validated commits were pushed to
[resurrection/m0-toolchain](https://github.com/bobby/suss/tree/resurrection/m0-toolchain)
without changing remote main. Migration commit: `8156c97`; callback/endpoint
probe commit: `64437bd`. The only unrelated untracked files remain the two
reference paths listed above.

The first remote prototype-baseline run is verified live:
[36620372831](https://github.com/bobby/suss/actions/runs/36620372831),
head `64437bda8a35fdafd3c8066865ce8c2c8cda5f40`, status in_progress, no conclusion
at observation. Poll with
`gh run view 36620372831 --repo bobby/suss --json status,conclusion,jobs,url`;
do not restart this job because observation times out. CI success is not claimed.
This CI observation describes the published head above. The later increment
below adds executing probes and carries this note with its implementation commit.
The optional Wasm compiler-component build remains a recorded failure.


## M0-02 canonical async import, guest cancellation and external-id — 2026-09-29

The two new original WAT fixtures execute canonical async lower and stackless
callback lift on Wasmtime 49.0.1:

- `callback-async-import.wat`: the host import returns Pending on its first poll,
  then completes with 42. The guest joins its subtask to a waitable set; its
  callback verifies the exact handle and RETURNED event. Rust verifies trace
  `[1, 2, 3]`, value 42 and at least two host polls.
- `callback-cancel-import.wat`: a polled, pending host import is cancelled by
  guest `subtask.cancel async` before joining the waitable set. The callback
  verifies RETURN_CANCELLED. Rust verifies trace `[1, 2, 3]`, acknowledgment marker
  99 and exactly one host-future Drop before Store teardown. The marker is a probe
  result, not an implementation of the Suss cancellation exception policy.
- WIT `@external-id` metadata survives wit-component embedding/encoding and is
  inspected at runtime to choose an exported function; invoking it returns 42.
  This is explicit host metadata interpretation, not implicit export renaming.

The async-import regression first failed because its artifact was missing.
The cancellation fixture initially failed validation with only component async
support enabled: async subtask.cancel requires the separate more-async-builtins
feature. The probe now explicitly selects
`Config::wasm_component_model_more_async_builtins(true)`, leaves stackful support
disabled and gives the guest 100,000 fuel. Both tests use the existing bounded
five-second driver. No production engine configuration changed.

Commands and results:

- `cargo test -p suss-compile --test toolchain_async canonical_async_import --locked -j2 -- --test-threads=2`: 1 passed.
- `cargo test -p suss-compile --test toolchain_profile wit_external_id --locked -j2 -- --test-threads=2`: 1 passed.
- `cargo test -p suss-compile --test toolchain_async guest_cancels --locked -j2 -- --test-threads=2`: failed validation before explicit feature selection, then passed.
- `cargo test -p suss-compile --test toolchain_async --test toolchain_profile --locked -j2 -- --test-threads=2`: 5 async + 8 profile tests passed, no ignored tests or failures.
- `rustfmt --edition 2024` on the two changed Rust test files: passed.
- `python3 -m unittest discover -s scripts -p 'test_*.py'`: 12 passed.
- `python3 scripts/wasi_lock.py`: verified 15 files and 6 packages.
- `git diff --check`: passed.

This increment adds feasibility tests only. The migrated native full workspace
baseline recorded above still applies; the new tests have separate passing
execution evidence. The existing remote CI run was observed still in_progress
with no conclusion; do not restart it on observation timeouts.

Limitations and next unblocked task: execute future/stream payload reads inside
guest memory, completion/EOF, bounded buffering/backpressure and cancellation of
pending endpoint operations. Top-level host call cancellation is still unavailable
through the public Wasmtime API; guest subtask cancellation does not establish
Suss interactive session interruption. Generated official WASI bindings, optional
Wasm compiler-component repairs and M1–M9 remain future work. M0-02 and milestone
M0 remain open until their full acceptance evidence is reviewable.


## Publication approval blocker — 2026-09-29

The executing async-import/cancellation/external-id increment is local commit
`3e8bad2`. Automatic approval review rejected both pushing it to the existing
`bobby/suss` branch `resurrection/m0-toolchain` and posting its result summary to
M0-02 issue #2. Its stated reason was lack of explicit trusted user authorization
for these external payloads/destinations. The repository origin was checked as
`https://github.com/bobby/suss.git`, private, with ADMIN viewer permission; the
review still rejected the push after those checks. No alternative publication
route was attempted. The remote branch retains `64437bd`; the new issue comment
was not sent. Ask the user to approve publishing the local increment and posting
the verified test summary to issue #2. Local implementation work is unblocked.


## M0-02 guest payload reads and endpoint cancellation — 2026-09-29

Four original fixtures extend the hand-written canonical ABI feasibility probes:

- `async-future-read.wat`: the producer returns Pending before delivering
  u32::MAX; the guest checks FUTURE_READ/COMPLETED and returns its memory load.
  Memory starts with a different sentinel to ensure a missing write cannot pass.
- `async-stream-read.wat`: four pending reads request exactly one byte each.
  The host produces 0, 255, 42 on the first three, then EOF with zero items.
  Guest callbacks assert the exact count/status, neighboring memory remains
  unchanged, and EOF leaves the whole sentinel unchanged. Independent Rust
  assertions check packed byte order, capacity one and no production before demand.
- `async-future-cancel-read.wat` and `async-stream-cancel-read.wat`: the guest
  cancels a pending read, verifies the endpoint-specific callback event and
  CANCELLED/zero-items result, then drops the read end. The producer poll trace
  starts with finish=false and ends with finish=true. Guest memory is untouched.

Commands/results:

- Focused future, stream and endpoint-cancel tests: passed individually.
- `cargo test -p suss-compile --test toolchain_async --test toolchain_profile --test shared_runtime --locked -j2 -- --test-threads=2`: 8 async + 8 profile + 1 shared-runtime passed, zero ignored/failures.
- `python3 -m unittest discover -s scripts -p 'test_*.py'`: 12 passed.
- `python3 scripts/cljs_inventory.py --check`: 1,065 declarations verified.
- `python3 scripts/wasi_lock.py`: 15 WIT files and six official packages verified.
- `python3 scripts/publish_roadmap.py`: offline preview, 10 milestones/39 stable issues; no remote writes.

### Local M0-02 acceptance audit

The issue's scope is selection/locking and executing feature feasibility, not
M5/M6's generated production interoperability suites.

| Acceptance requirement | Authoritative executing/locked evidence |
| --- | --- |
| Core GC, tail calls, exceptions | `rust_engine_executes_required_core_features` executes each and returns 42; typed function references also execute |
| Maps | `canonical_map_values_cross_guest_memory_in_both_directions` executes empty and Unicode-key maps with u32::MAX through canonical memory |
| implements/external-id | Named implements import invokes the Rust host; external-id survives WIT encoding and selects an executing export |
| Async functions | Callback YIELD/resume, canonical lower actually Pending/complete, guest subtask cancellation |
| Future | Read-end transfer without implicit await; independent host consumption; pending guest-memory payload read and read cancellation |
| Stream | Read-end transfer; independent boundary-byte consumption; guest-memory one-byte demand/order, EOF and read cancellation |
| Exact versions | Cargo.toml/Cargo.lock pin Wasmtime 49.0.1, wasm-tools family 0.258.0 and wit-bindgen 0.61.1; focused runtime tests execute that lock |
| Official WIT hashes | `wasi-wit-lock.json` pins release/archive/file hashes; offline verifier checks 15 files/six packages; original package versions resolve in the executing profile suite |
| Report unsupported individually | Prototype rejects async exports and map/future/stream/error-context boundary adapters explicitly; public top-level host cancellation is unavailable; optional Wasm CLI compiler-component build fails native-only imports |

All named local feasibility requirements now have passing evidence. Issue #2 and
milestone M0 remain open; the external publication blocker above is unchanged.
The remote CI run on earlier head 64437bd was still live/in_progress at 19:54 UTC
and is not evidence for these unpublished tests. Do not restart it on observation
timeouts.

Limits: these are hand-written feasibility artifacts. No production Suss
adapters/scheduler were implemented. Nested types/resources, arbitrary buffering,
zero-length readiness reads, cancellation races/retry/write cancellation and
lifetime/stress coverage remain the M5/M6 suites. No complete WASI alpha or M0
milestone claim follows from this local M0-02 audit.

Next unblocked work: audit M0-03's shared GC fragment acceptance on the selected
engine and M0-01's inventory/license gates, then finish M0-04's browser feature
profile and optional Jco comparison. Continue local implementation while remote
publishing awaits explicit approval.


## M0-03 shared GC fragment acceptance — 2026-09-29

The previously passing fixture compared an ABI version but did not demonstrate
that incompatible fragment initialization is prevented. It now uses a probe-only
loader that checks the fragment's declared ABI before engine instantiation. Two
negative execution probes verify:

- Version mismatch rejects the fragment with the precise ABI diagnostic and
  leaves its imported effect global zero. Matching ABI initializes it to 99.
- A separately validated fragment with a different recursive descriptor field
  layout (i64 instead of i32) fails linking despite a matching version label.
  Its start effect also remains zero.

The shared-GC fixture additionally captures the original closure, redefines the
shared binding and forces GC. A lookup returns 104 while the retained capture
returns 44; the earlier object's fields and descriptor identity also survive GC.

Commands/results:

- `cargo test -p suss-compile --test shared_runtime --locked -j2 -- --test-threads=2`: initially compile-failed before the new probe loader existed; after implementation, three passed.
- A deliberate temporary bypass of the ABI gate caused `incompatible_fragment_abi_is_rejected_before_start_effects` to fail (exit 101, instantiated instead of rejecting). The original source was restored in a finally block; the focused suite then passed again. No bypass remains.
- `cargo test -p suss-compile --test shared_runtime --test toolchain_profile --test toolchain_async --locked -j2 -- --test-threads=2`: 3 shared + 8 profile + 8 async passed, zero ignored/failures.
- The changed Rust test file was rustfmt-formatted; `git diff --check` passed.

Local acceptance audit:

| M0-03 criterion | Evidence |
| --- | --- |
| One Store shares recursive GC types across fragments | Runtime/A/B are separately compiled, use identical recursive groups and instantiate in one Store |
| Roots survive forced GC | Only runtime globals retain closures/objects; GC before loading B and after nominal redefinition preserves callable closures and old fields |
| Closures cross fragments | A's function reference/environment are invoked by B through call_ref |
| Nominal descriptors | Same layout and numeric descriptor payload still have different ref.eq identities; old object's descriptor survives |
| Incompatible ABI rejected | Declared version check runs before instantiation/start; engine rejects incompatible recursive layout with matching label |

All local M0-03 named feasibility criteria have evidence on Wasmtime 49.0.1.
Issue #3 and milestone M0 remain open; remote publication is still pending
explicit authorization after automatic review rejected the earlier push/comment.
The three new tests only change feasibility infrastructure. They do not implement
production artifact manifests, stable language binding-cell layouts, source spans,
macro phases or the persistent compiled REPL, and do not change the accepted design.

Next unblocked task: audit M0-01's deterministic inventory and source/license
policy, then finish M0-04's complete browser feature profile and optional Jco
comparison. Keep implementation, evidence and stable GitHub issue IDs aligned.


## M0-01 inventory review schema and provenance gate — 2026-09-29

The existing generated inventory had deterministic form/range/hash data, but
review schema validation existed only as prose. `scripts/cljs_reviews.py` now
parses a strict EDN data subset, never executes forms, and validates the overlay
against freshly read pinned declarations. It fails on stale commit/form hashes,
unknown/duplicate IDs, missing/unknown fields, invalid visibility/classification,
malformed dependencies/arities, implementation claims without test references,
and exclusions without an individual host-specific rationale/alternative.
Function/macro reviews require explicit arities; noncallable declarations may
use nil. Evidence references are not treated as passing test results.

`inventory_records()` exposes the same pinned source records to generation and
review validation without changing generated output. CI now invokes the review
validator after the inventory check. Eight focused overlay tests cover malformed
and stale data, while the five existing scanner tests remain passing.
`docs/compatibility/PROVENANCE.md` specifies the origin/extraction/patch hashes,
retained notices and corresponding license packaging needed before future core
imports. Existing pinned upstream source notices and license texts were inspected;
no upstream core forms were ported in this increment.

Commands/results:

- `python3 -m unittest discover -s scripts -p 'test_*.py'`: 20 passed.
- `python3 scripts/cljs_inventory.py --check`: 1,065 declarations verified.
- Two independent `generate()` calls: identical bytes/count, SHA256
  `b6f3bce5e2847efd0eabee1b61c83914be1c659f41c6f51b04fdc4bcb8509f8c`.
- Phase/branch audit: 811 runtime, 254 macro declarations, both :clj/:cljs labels.
- `python3 scripts/cljs_reviews.py`: 0 reviewed, 1,065 unassessed.
- CLI negative verification with a temporary overlay containing a stale source
  hash: exit 1 and the expected stale-hash diagnostic. No review was added.
- WIT lock verification passed; roadmap publisher offline preview retained
  10 milestones/39 issue IDs; `git diff --check` passed.

Local M0-01 acceptance now has evidence for deterministic regeneration,
source hashes/reader branches, a machine-checked portable API review schema and
the source/license policy. Issue #1 and M0 remain open pending publication and
remote acceptance updates. This is not completed classification, a complete
public API inventory, core import provenance automation or portable compatibility.
Those remain M4/M7 acceptance work. Rust runtime code did not change, so this
increment validates the inventory tooling rather than repeating its prior full
native Rust baseline.

## Terminal result of first remote baseline — 2026-09-29

Run [36620372831](https://github.com/bobby/suss/actions/runs/36620372831), head
64437bd, is now authoritatively completed/cancelled. Job 109583927141 started
19:35:57 UTC and ended 20:01:13 UTC. Inventory/scanner/WIT lock checks passed.
The Rust command compiled in 3m56s, then reached compile_expr and was cancelled
while those tests were still running; the log ends with `The operation was
canceled.` The elapsed job time matches the configured 25-minute budget. No
complete remote test pass or specific semantic failure follows from partial
passing expression tests. The run was not restarted. Full logs were read locally
from `/tmp/suss-ci-36620372831.log`; the GitHub run is the durable reference.

The later local commits have not been published and are not covered by that run.
Publishing and issue comments still await explicit authorization after automatic
review rejected the earlier actions. Local work is unblocked.

Next unblocked task: address M1-03's bounded Linux test runtime from this observed
cancellation (inspect profiles and repeated prototype compilation), then finish
M0-04's browser feature profile and optional Jco comparison. Do not fix CI by
skipping semantic tests or replacing missing results with success.


## Publication authorization restored — 2026-09-29

The user explicitly authorized committing, pushing implementation branches and
opening PRs in the Suss GitHub repository as needed for the goal. The previous
publication blocker is resolved. Publish the validated local increments to the
existing `resurrection/m0-toolchain` branch and open a PR against main. Record
results on the stable roadmap issues. Do not merge or mark the full roadmap/M0
complete; bounded remote CI and the M0-04 browser profile still require work.
