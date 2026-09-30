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


## Implementation branch and draft PR published — 2026-09-29

All validated local increments through aa3057a were pushed to
`resurrection/m0-toolchain`. Draft [PR #40](https://github.com/bobby/suss/pull/40)
now targets main; it has not been merged. Progress/evidence comments were posted
on stable issues [M0-01](https://github.com/bobby/suss/issues/1#issuecomment-5897837810),
[M0-02](https://github.com/bobby/suss/issues/2#issuecomment-5897839210),
[M0-03](https://github.com/bobby/suss/issues/3#issuecomment-5897840444) and
[M1-03](https://github.com/bobby/suss/issues/7#issuecomment-5897841862).
The publication approval blocker is resolved. Issues/milestones remain open
while their acceptance results are reviewed and remaining work is implemented.

Both push run 36624215715 and PR run 36624292520 started on aa3057a. A cancellation
request for the duplicate push run was accepted; the last observation still
reported in_progress, so completed cancellation is not claimed. Keep the PR run
[36624292520](https://github.com/bobby/suss/actions/runs/36624292520), observed
live/in_progress with no conclusion. This is a new run triggered by new published
coverage, not a restart due to an observation timeout. Avoid duplicate push/PR
runs in the next bounded-CI implementation. Next: inspect unoptimized test
profiles and repeated prototype/Wasmtime compilation to repair Linux runtime
without skipping cases. Include this local publication note with that change
rather than trigger another CI run solely for a status note.


## M1-03 test dependency optimization — 2026-09-29

The cancelled Linux run compiled successfully in about four minutes, then spent
its remaining budget executing prototype expressions. A temporary phase-timing
probe for test_addition was applied to the existing run_expr_i32 helper and
restored in a finally block after each measurement. The same arithmetic result
passed before and after; no profiling instrumentation remains in the source.

| Local phase | Unoptimized dependencies | Optimized dependencies |
| --- | --- | --- |
| Suss source compilation | 208.7 ms | 168.9 ms |
| Engine/Wasmtime module compilation | 806.0 ms | 80.8 ms |
| Instantiate/call | 0.57 ms | 0.41 ms |
| Focused test total | 1.02 s | 0.25 s |

This is a single local timing probe, not a Linux or whole-suite performance
claim. `profile.test.package."*"` now uses Rust opt-level 2 for non-workspace
dependencies, with debug assertions and overflow checks explicitly enabled.
Workspace code remains at its ordinary unoptimized test profile. Debug symbols
and incremental caches remain disabled. Cranelift's Wasm optimization setting,
source semantics, case expectations and test selection do not change. No
RUSTFLAGS was set by a command.

CI now runs pushes to main and PR events, avoiding duplicate branch push/PR
runs. A per-PR/ref concurrency group cancels superseded heads. Build jobs remain
two, test threads remain two, job timeout remains 25 minutes, and CI uses the
exact full baseline command `cargo test --workspace --locked -- --test-threads=2`.
YAML parses successfully and the bounded job/command were checked.

The focused optimized build/test passed after 3m55s of dependency rebuilding.
The full workspace baseline completed with exit 0 under the changed profile:
`CARGO_BUILD_JOBS=2 cargo test --workspace --locked -- --test-threads=2`.
Its additional CLI/WASI feature union rebuilt artifacts in 3m57s. Results:
14 CLI, 54 compiler units, 316 expression (12 existing ignored), 28 component,
4 conformance harness (2 manual recorders ignored), 3 shared-runtime, 8 async,
8 profile, 8 core and 19 reader tests passed; two existing doc examples remain
ignored. No failures or new skips. The strict conformance baseline still checks
all 201 reviewed cases. Log: `/tmp/suss-optimized-baseline.log`.

Expression execution fell from 319.67s in the migrated baseline to 55.00s;
conformance from 192.07s to 21.49s. These are local observations. The optimized
Linux PR run must finish before claiming a repaired remote baseline. The earlier
PR run on aa3057a is separate evidence; the duplicate push run's cancellation
is authoritatively completed/cancelled. Twenty Python checks and inventory,
review-overlay and WIT lock verification pass. No baseline expectations changed.
Next: publish this profile/workflow change, observe the new PR run and retain
M1-03 open until its full acceptance is proved. Continue M0-04 browser/Jco work
while CI runs.


The optimized profile/workflow is published as a070a5e in PR #40. New PR run
[36626065207](https://github.com/bobby/suss/actions/runs/36626065207) is verified
live/in_progress on that exact head; no duplicate branch-push run was created.
The superseded aa3057a PR run lacks the new concurrency group, so cancellation
was explicitly requested only after the successor was confirmed live. Its
terminal result is not yet claimed. Follow the new handle without restarting it
on observation timeouts. Include this post-push observation with the next code
increment instead of pushing a status-only commit and triggering extra CI.


## M0-04 core feature profile and optional Jco route — 2026-09-29

The browser fixture now executes the required GC/function-reference/tail-call/EH
WAT probes, each returning 42, before the existing Promise/GC-continuation,
cancellation and stale callback checks. `instantiateRequired` preserves the
feature name and engine CompileError cause; invalid bytes explicitly exercise
that failure path. It does not classify every compilation error as unsupported
syntax. The minimal bridge still has signed-i32 fixture values, not the future
Suss f64/UTF-16 runtime contract.

The first run was sandbox-blocked from binding localhost. The authorized isolated
headless harness was then rerun outside that restriction. Its first expanded run
returned RUNNING instead of a complete DOM result; it failed, not skipped. Tiny
probe compilation now uses synchronous WebAssembly.Module/Instance constructors
within the async initialization helper to avoid virtual-time racing background
compilation. The full fixture subsequently passed.

Optional Jco comparison: exact @bytecodealliance/jco 1.35.0 was installed under
/tmp with --ignore-scripts; npm reports Apache-2.0 WITH LLVM-exception and integrity
`sha512-O53cWIMm/M1SpwLxKRicvAMDnzWf4zWs6I6qFZIGbn3HUovDMCCe9blkGxxYuyocHjVf/cXgNmlMu/x7Tyi77A==`.
Node v24.5.0/npm 11.5.1 are development tools only. The original
`jco-gc-component.wat` is transpiled with explicit async instantiation and browser
module loading, then its generated GC core actually executes and typed export
returns 42. Generated JS/Wasm/declaration hashes are recorded; generated third-
party runtime code is not vendored or shipped. The temporary installation lock
is /tmp/suss-jco-1.35.0/package-lock.json. Future production packaging requires
its own dependency/license lock and the full M8 suite.

The Python result validator now requires all four feature labels and every exact
acceptance check. Requested Jco results cannot be absent, partial or default to
success. Five new malformed-result tests cover those failures. Raw browser output
and validated semantic status are separate. Timeout teardown no longer creates
an invented zero exit code: evidence preserves null and the explicit termination
flag. Passing DOM proves the semantic fixture only, not clean Chrome shutdown.

Commands/results:

- `python3 -m unittest discover -s scripts -p 'test_*.py'`: 25 passed.
- `node --check` for bridge, direct probe and Jco probe modules: passed.
- `python3 scripts/probe_browser.py --chrome '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome' --wasm-tools /tmp/suss-toolchain-49/wasm-tools-1.258.0-aarch64-macos/wasm-tools --jco /tmp/suss-jco-1.35.0/node_modules/.bin/jco --output docs/roadmap/browser-probe.json`: executing semantic/packaging probes passed; Chrome teardown required termination after complete DOM output.

Local acceptance audit: supported browser/core module loading, typed result 42,
Promise suspend/resume, required feature failure diagnostics and direct/optional
Jco routes now all have executing evidence. This does not establish cross-browser
support, arbitrary browser/WIT shapes, canonical async values or compiled Suss
browser output. Issue #4 and milestone M0 remain open during acceptance review.
The native Rust baseline from a070a5e remains applicable; this increment changes
browser/tooling fixtures only. The optimized Linux CI handle 36626065207 was
observed still live/in_progress, not a terminal pass, while these tests ran.

Next unblocked task: observe that same CI handle to finish M1-03 evidence, audit
M1-01 strict decoder gates, and implement the pinned ClojureScript development
oracle (M1-02) before replacing foundation/runtime semantics in M2.


The browser increment is validated for a local commit. Hold its branch push
until optimized Linux run 36626065207 becomes terminal: publishing a new PR head
would cancel that live validation and prevent its successful cache save. This
is a resource/evidence decision, not a permission blocker. User authorization to
commit/push/open PRs remains in force. The existing PR #40 head is still a070a5e;
include the new browser coverage and update issue #4/PR when that run finishes.


## M1-01 malformed evidence and executed decoder failures — 2026-09-29

Audit found that unknown case fields were ignored, namespaced schema keys were
reduced to unqualified names, and non-Boolean skip values passed validation.
The case loader now rejects those inputs, empty names/expressions, invalid
categories and duplicate IDs/fields; missing files are explicit errors rather
than an empty suite. The existing source corpus/catalog/expectations are unchanged.

Baseline/catalog JSON uses a unique-key map decoder; serde_json's default map
behavior would discard duplicate keys. Serde is now a direct development-only
dependency, already present at the same locked version; the Cargo.lock diff adds
only that dependency edge. Exact failure comparison regressions reject new
failures, changed stage/diagnostics and unexpected passes. Executed custom Wasm
GC fixtures reject unknown i31/struct tags, malformed float boxes and raw non-string
arrays; no opaque value is accepted as a wildcard.

Validation:

- `cargo test -p suss-compile --test conformance --locked -j2 -- --test-threads=2`: 8 passed, 2 manual evidence/catalog recorders ignored, 20.39s. The strict baseline executes all 201 reviewed cases with zero known failures/skips.
- A temporary bypass of field validation causes the malformed-case regression
  to fail (exit 101); source was restored in a finally block and the focused test
  passes afterward. No bypass remains.
- `rustfmt --edition 2024` on the changed harness and `git diff --check`: passed.

Local M1-01 acceptance now has executing regressions for wrong values/collections,
malformed/missing cases, unknown layouts and exact baseline changes. This does
not establish the future lossless ClojureScript oracle, f64/UTF-16 runtime contract,
complete portable API coverage or effect traces; those remain M1-02/M2/M7 work.
Native production code did not change. The new harness executes independently
under the already-verified optimized profile; its known-failure/catalog files
were not regenerated to hide changes.

Linux run 36626065207 remains confirmed live on a070a5e. Browser commit 22c8fc1
and this harness increment are queued locally; hold publication until that run
is terminal to preserve the current validation/cache work. User push/PR
permission remains in force. Next unblocked implementation: pinned ClojureScript
development oracle and lossless tagged value/effect corpus (M1-02).


## Linux baseline terminal result and pinned oracle transport — 2026-09-29

Optimized Linux run [36626065207](https://github.com/bobby/suss/actions/runs/36626065207)
on PR #40 head a070a5e is completed/success. The full downloaded job log confirms
14 CLI, 54 compiler, 316 expression (12 existing ignored), 28 component,
4 conformance (2 manual recorders ignored), 3 shared-runtime, 8 async, 8 profile,
8 core and 19 reader tests passed; 2 existing doc examples remain ignored.
Build: 11m22s; expressions: 377.66s; conformance: 205.89s. The job finished
within the existing 25-minute budget, then uploaded all 411,109,289 cache bytes.
This is Linux evidence for a070a5e, not the newer browser/conformance/oracle
commits. It resolves the earlier live-run publication hold; those increments can
now be pushed under the user's explicit branch/PR authorization. The full local
baseline and this remote run establish the current bounded baseline evidence;
M1-03 remains open for acceptance review, with no future milestone completion.

The new original development oracle compiles the clean pinned ClojureScript
submodule (c4295f303100bbf5afac449242d30bca1126f1a1, 1.12.134) using explicitly
pinned Clojure 1.12.1 and executes it in Node. The transport preserves f64 bits
(including signed zero, NaN/infinity), UTF-16 code units (including a lone
surrogate), collection kinds, exception data/message and ordered effect labels.
The strict Python validator rejects missing/duplicate/unknown fields and IDs,
unknown layouts, changed boundaries, reordered effects and serializer exceptions
substituted for required values. Snapshot provenance and limitations are in
`tests/oracle/README.md`; no upstream core source was ported in this increment.
Java/Node are development-only tools, not shipped dependencies.

Commands/results:

- `scripts/test-oracle.sh`: passed; 12 reference observations verified. Clojure
  CLI 1.12.1.1550 / Java 21.0.2 / Node v24.5.0. Cold Maven downloads required
  network escalation; task-specific configuration/Maven caches stay under /tmp.
  A first Node invocation from the repository root failed on relative output
  paths; the runner now changes to tests/oracle before build and execution.
- `python3 scripts/oracle_transport.py tests/oracle/out/observations.json`: passed
  after the final validator changes. Output explicitly disclaims a Suss pass.
- `python3 -m unittest discover -s scripts -p 'test_*.py'`: 32 passed (7 new
  malformed/reference transport regressions). CI includes these Python tests.
- `gh run view 36626065207 --repo bobby/suss --log`: completed/success job log
  downloaded to /tmp/suss-ci-optimized-success.log and suite/cache results audited.

M1-02 remains partial: the source corpus must be shared with Suss, an independent
lossless Suss result decoder/comparator must execute, and failure stages and
semantic differences must be recorded exactly. The reference fixture cannot
establish the current integer/UTF-8 runtime's binary64/UTF-16 conformance. JVM/
Node compilation is not yet a CI gate. No full Rust rerun is needed for this
pure development transport increment; d3d476e's focused conformance execution
and the prior full baselines remain the relevant production/harness evidence.
Next unblocked task: implement the shared differential corpus and independent
Suss observation path for M1-02, preserving unsupported cases as honest failures
before M2 representation changes.


## M1-02 shared differential corpus and exact observations — 2026-09-29

The previous goal turn was progress: three validated commits were published to
PR #40 through 37d7c65 and issues #6/#7 updated. CI run 36629536970 on that head
is authoritatively still in_progress; it restored cache and passed Python/
inventory gates, with the Rust baseline running. Do not cancel/restart it or
claim its eventual result. Publication of this new increment is held until that
specific run becomes terminal to preserve useful validation/cache work.

`tests/oracle/cases.json` now supplies the exact expression bodies to both
runners. The original ClojureScript serializer is unchanged; Python generates
thunks under ignored out/generated. Rust compiles each identical body inside a
trace-returning wrapper, validates and executes the Wasm with a reused engine,
20-million fuel bound and two threads, then independently reads the GC heap.
Host encoding preserves float bits and UTF-16 code units. Prototype integers
convert only if exact; a focused regression rejects 9007199254740993 instead of
rounding it to the reference value. Unknown values remain decoder failures.

The comparator preserves collection kinds and numeric bits (including NaN
payloads), matches maps/sets without ordering but with one-to-one element use,
and compares effects in exact order. Malformed/missing results fail rather than
becoming a tracked success. The initial empty failure baseline FAILED (exit 101),
proving that newly observed differences fail the gate. Observations were captured
via an explicit manual test and every failure reviewed before recording this
new corpus's initial baseline; existing legacy expectations were not changed.

Actual initial differential result: **5 passing, 7 failing, 0 skipped**:

- binary64-rounding: actual bits 3ff0000000000000 (1) instead of 4340000000000000
  (9007199254740992).
- negative-zero: actual positive-zero bits instead of 8000000000000000.
- utf16-surrogate and utf16-pair: compile-stage parse failure, exact diagnostic
  `Parse error: found end of input expected something else`.
- nested-values: the quoted seq is decoded as a vector, while maps/sets and the
  other nested values compare correctly independent of map iteration order.
- variadic-arity: rest arguments are a vector instead of a seq.
- exception-effect: compile-stage `Undefined symbol: ex-info`.

These are newly exposed prototype incompatibilities in a new corpus, not approved
regressions in the prior baseline. `tests/oracle/known-failures.json` retains exact
expected/actual tagged output or stage/diagnostic. Changed failures, new failures
and unexpected passes fail; stable failures are expressly not compatibility.
The old 201-case catalog still has zero known failures and no skips.

Commands/results:

- `scripts/test-oracle.sh`: passed with a freshly compiled Node reference and
  executing Suss comparison; 5 differential passes, 7 exact known failures.
- `cargo test -p suss-compile --test oracle --locked -- --test-threads=2` with two
  build jobs: 2 tests passed, 1 explicitly manual capture ignored, ~1.1s.
- `SUSS_ORACLE_OUTPUT=/tmp/suss-shared-observations.json cargo test -p suss-compile --test oracle --locked record_observations -- --ignored --exact --test-threads=2`
  with two build jobs: captured actual observations; never writes expectations.
- `python3 -m unittest discover -s scripts -p 'test_*.py'`: 39 passed, including
  seven new corpus/comparator regression methods for malformed inputs, collection
  matching, numeric/effect differences and exact stage/diagnostic changes.
- `cargo test --workspace --locked -- --test-threads=2` with CARGO_BUILD_JOBS=2:
  passed; full log /tmp/suss-shared-oracle-full.log. New differential suite is
  included; stricter legacy conformance has 8 passes/2 manual ignored. Existing
  ignores are retained; no new semantic cases are skipped.
- `rustfmt --edition 2024` and `git diff --check`: passed.

M1-02 remains open. The observation wrapper currently cannot recover independent
exception data/message or partial traces from failed execution, and ex-info is
not implemented. Java/Node remain development-only; ordinary Cargo/CI runs use
only the checked-in reference fixture and Python comparator. No architecture or
production core forms were ported; source provenance is original repository code.
Next unblocked task: add independently decoded caught-exception observations and
focused exception/effect cases, then use the exact differential evidence to drive
the M2 binary64/UTF-16 and closure/sequence foundation repairs.


## Caught exceptions, cleanup repair and terminal Linux CI — 2026-09-29

The previous turn was progress (local effb7fe shared differential implementation).
CI run [36629536970](https://github.com/bobby/suss/actions/runs/36629536970) on
published 37d7c65 is now completed/success, verified through the full downloaded
log /tmp/suss-ci-37d7c65-success.log. Cached build took 17.41s; 316 expression
passes/12 existing ignored took 383.27s; stricter conformance has 8 passes/2 manual
ignored in 208.37s. All other enabled suites passed. This resolves the publication
hold for effb7fe; it does not establish CI success for these newer commits.

Transport schema 2 adds the exact thrown value to exception observations and a
recursive ExceptionInfo data/message/cause tag. Old schema 1 and missing thrown
values fail validation. The pinned ClojureScript serializer now records thrown
nil, strings and maps, plus body/cleanup effects on a throwing finally path.
No upstream implementation was copied: the pinned core ex-data/ex-message source
was consulted to establish that non-Errors return nil; original host transport
retains the exact thrown object separately. Actual ExceptionInfo still fails at
Suss compilation (undefined ex-info), exactly as before.

Suss's observation wrapper catches language throws into a Boolean-discriminated
outcome, then independently decodes the thrown value and partial effect trace.
Nil, false and zero cannot be mistaken for normal returns. Wasmtime Trap errors
are classified separately as trap failures, not caught language exceptions.
Executed fuel exhaustion proves that separation; failed decoding or traps still
cannot expose a partial trace through this prototype wrapper.

The new finally differential case FAILED before a production repair: cleanup was
absent from the trace. Focused source regression also failed (exit 101):
`(try 42 (finally 7))` returned nil/decoded 0 instead of 42. Lowering had treated
finally-only as a plain block, discarded the body result and skipped cleanup on
throw; user catch throws also bypassed cleanup. It now guards body plus user catch
with a private catch: cleanup precedes rethrow on the exceptional path, and the
existing normal-result path preserves the value while running cleanup. Cleanup
throws supersede prior exceptions; cleanup executes once. This uses the existing
IR/backend exception machinery, not a new runtime architecture.

Focused regressions execute normal return, body throw, catch-body throw, cleanup
throw and exact-once cleanup. The new four differential cases all pass after the
repair. Current shared corpus: **16 cases, 9 passing, 7 failing, 0 skipped**.
The same seven known failures remain byte-for-byte unchanged; no finally failure
was accepted into a green baseline. The 201-case legacy baseline is unchanged.

Commands/results:

- Focused finally regression before repair: exit 101; after repair: passed.
- `scripts/test-oracle.sh`: fresh pinned reference and Suss execution passed;
  16 reference observations, 9 differential passes/7 unchanged known failures.
- `cargo test -p suss-compile --test oracle --locked -- --test-threads=2` with two
  build jobs: final suite has 4 passes/1 manual capture ignored. Captured values
  include nil/false/zero/string/map plus partial effects; fuel trap is distinct.
  The initial trap fixture had compile-stage failures (zero-binding loop and
  function recur are not supported); a supported bound loop now actually exhausts
  the 20-million fuel budget and passes the trap assertion, with no skip/default.
- `python3 -m unittest discover -s scripts -p 'test_*.py'`: 41 passed; new checks
  reject old schema, incomplete ExceptionInfo and omitted/changed thrown values.
- `cargo test --workspace --locked -- --test-threads=2` with CARGO_BUILD_JOBS=2:
  passed; expression suite 317 passes/12 existing ignored (53.90s), stricter
  conformance 8 passes/2 manual ignored (21.77s). Full log
  /tmp/suss-finally-full-baseline.log. The fuel-only regression was finalized
  afterward and passed in the focused oracle suite; production code is unchanged
  since the full run. No unrelated files or ignores were removed.
- `rustfmt --edition 2024` on the oracle harness and `git diff --check`: passed.

M1-02 remains open for acceptance review and broader semantic/arity coverage.
Numeric/UTF-16, sequence/rest representation and ExceptionInfo incompatibilities
remain explicit M2/M4 foundation work, not reasons to weaken the contract.
Next unblocked task: implement the accepted shared ABI v1 value representation
and reader/IR boundaries in M2, starting with lossless binary64 and UTF-16,
using these differential failures as acceptance tests. Preserve the persistent
fragment/closure constraints already demonstrated in M0.


## Linux canonical NaN sign regression — 2026-09-29

The previous turn was progress: 5b0b5e2 and effb7fe were published to PR #40.
Its new run [36631941576](https://github.com/bobby/suss/actions/runs/36631941576)
is terminal/completed/failure, not a live wait or a success. Full failed-step
logs are /tmp/suss-ci-5b0b5e2-failed.log. Linux observed NaN bits fff8000000000000
for zero/zero, while the Node fixture recorded 7ff8000000000000. Every other
observed failure matches the seven reviewed failures exactly; Rust suites before
the oracle passed. The comparator incorrectly required one canonical NaN sign.

WebAssembly permits either sign for a canonical arithmetic NaN; see
https://www.w3.org/TR/wasm-core/ and the dated evidence clarification in the
accepted design. The new regression FAILED before the comparator repair. The
comparison now permits only this canonical NaN sign difference. Raw transport
bits remain intact; changed payloads, finite values and signed zero still fail.
Storage/boundary tests continue to require exact NaN bits. No NaN failure was
added to known failures and no unknown/opaque result is accepted as success.

Validation: 42 Python tests passed. The exact Linux failure JSON was audited:
removing its canonical-sign-only NaN observation leaves the seven expected
failures byte-for-byte equal, and that raw NaN observation passes the corrected
comparison. This is an offline diagnostic replay, not a new green Linux job.
The final full workspace baseline also passes, including the uncommitted shared
runtime foundation (seven additional runtime tests), with 9 differential passing,
7 failing and 0 skipped. New-head CI must execute after publication.

Publish this focused correction to PR #40 first; the new M2 runtime implementation
will be a separate stacked branch/PR based on that head. Neither milestone is
closed. No shipped Java/Node path or production arithmetic was changed here.


## M2-03 generated shared runtime foundation — 2026-09-29

The original production `runtime_abi` module now generates a shared GC ABI v1
runtime with wasm-encoder; it is not another reader/source compiler and does not
replace the legacy backend yet. `docs/runtime/abi-v1.md` records all ten recursive
layouts, intrinsics, manifest checks and integration limitations. No upstream
core implementation was copied; repository license applies. No dependency or
Cargo.lock change, no Java/Node or target host imports, no linear-memory ABI.

Real generated Wasm implements boxed f64 arithmetic/storage, packed UTF-16 unit
storage, i31 nil/Boolean sentinels, universal closures with minimum/maximum arity
(including variadic), initialized binding cells and a rooted built-in error
descriptor. Wrong arity throws a language exception caught by an independently
loaded module, with UTF-16 diagnostic independently decoded. Old captured closure
values survive forced GC and binding replacement while current cell lookup sees
the replacement. Scalar types canonicalize across generated runtime, producer
and consumer modules. Lone surrogates/astral pairs stay distinct code units;
invalid unit/index writes are rejected without mutation or truncation.

Manifest verification records compiler package/runtime ABI/wasm-tools versions,
rejects malformed/duplicate/absent/mismatched records, and compares the actual
explicit recursive group against the generated prelude. It checks before any
initializer. A new regression changed the Number layout to valid i64 while
retaining matching metadata: the old version-only check FAILED the regression
because initializer effects ran. The strengthened prelude check now rejects
before effects. Function semantics still require engine validation/linking and
executing tests; version metadata is not a blanket proof of compatibility.

Validation:

- Numeric runtime regression initially failed (missing runtime export), then
  passes on actual generated module. A test fixture first needed an explicit
  f64 type annotation; that compile failure was corrected before semantic proof.
- `cargo test -p suss-compile --test runtime_abi --locked -- --test-threads=2`
  with CARGO_BUILD_JOBS=2: final 7 passed/0 ignored, ~0.03s. Heap fields/units are
  inspected independently; boxed NaN payload and signed-zero bits are exact.
  Arithmetic covers add/subtract/multiply/divide/negate and binary64 rounding;
  arithmetic NaN class is checked separately from exact boxed-bit storage.
- Manifest/prelude mismatch regression: failed before strengthening, passed
  afterward, with incompatible initializer effect count 0 and compatible count 1.
- `cargo test --workspace --locked -- --test-threads=2` with CARGO_BUILD_JOBS=2:
  final pass, /tmp/suss-abi-v1-final-full.log. 317 expressions/12 existing ignored
  in 53.27s; strict legacy conformance 8 passes/2 manual ignored in 20.49s;
  oracle 4 passes/1 manual ignored; 7 new runtime ABI passes/0 ignored. Shared
  source corpus stays 9 differential passing/7 exact known failures/0 skipped.
- Python suite: 42 passed. `rustfmt --edition 2024` for new runtime/harness and
  `git diff --check`: passed. Existing source files/legacy ignores are retained.

Commit/publish on resurrection/runtime-abi-v1, with a stacked PR based on
resurrection/m0-toolchain (8fa7a63 canonical-NaN correction). User branch/PR
permission remains in force. M2-03 is in-progress, not complete: reader/HIR/IR
lowering still uses prototype integer/UTF-8 layouts; the source float/UTF-16
cases do not yet exercise these new intrinsics. Existing CLI/AOT artifacts and
loaders have not migrated to the manifest gate. Complete protocols/nominal types,
ExceptionInfo/dynamic scope, unbound-var diagnostics, persistent REPL, scheduler,
target adapters and publishing metadata remain their original roadmap work.

Next unblocked task: migrate the reader/IR value boundary to lossless binary64
and UTF-16, then lower source expressions into this shared runtime while retaining
explicit operand order. Wire the version/prelude gate into fragment loading;
retire prototype code only after its replacements pass the relevant corpus.


## Dispatched PR #40 code review — 2026-09-29

The user requires an independent dispatched subagent review for every PR, fixes
for significant findings pushed to that PR, and passing CI at the final head.
Do not merge PRs yet. PR #41 is stacked on #40 and must incorporate this base
review commit before its final validation.

The PR #40 reviewer audited the complete diff against the accepted design:
production analyzer/lowerer/codegen migration and evaluation order, cleanup and
arity changes; independent decoders and exact conformance/oracle gates;
inventory/review-schema and upstream licenses/source locks; executing shared-GC,
canonical async/map/implements/external-id probes; browser evidence tooling and
bounded CI. Feasibility fixtures and seven explicitly tracked differential
failures remain partial evidence, not completed runtime or release claims.

Two significant findings were reproduced and fixed:

- WIT-exported catch/finally code used unshifted logical catch locals, although
  flattened canonical parameters precede language locals. With an option and
  scalar parameter, compilation failed final component validation: expected i32,
  found eqref. The emitter now applies the inherited parameter offset to all
  catch-local writes and result restoration. An executing component regression
  covers caught values and normal finally results with both None/Some inputs.
- Legacy strict conformance classified fuel traps as ordinary execution failures,
  obscuring the distinction from uncaught language exceptions. An executing
  regression initially reported execution instead of trap. It now downcasts
  Wasmtime Trap for the stage and separately verifies a language throw remains
  an execution failure. No known-failure files or case expectations changed.

Focused commands/results:

- `cargo test -p suss-compile --test component --locked execute_wit_export_catch_and_finally_with_flattened_params -- --exact --test-threads=2`: failed before offset fix (exit 101), passed after.
- `cargo test -p suss-compile --test component --locked -- --test-threads=2`: 29 passed.
- `cargo test -p suss-compile --test conformance --locked runtime_traps_are_distinct_from_uncaught_language_exceptions -- --exact --test-threads=2`: failed before stage fix (exit 101), passed after.
- `python3 -m unittest discover -s scripts -p 'test_*.py'`: 42 passed.
- Inventory check: 1,065 declarations unchanged; review overlay 0 reviewed/1,065
  unassessed; offline WIT verification 15 files/6 packages; roadmap preview 10
  milestones/39 issues. No unsupported cases or unknown evidence became success.

All Cargo commands use CARGO_BUILD_JOBS=2 and the existing shared target directory;
no RUSTFLAGS override. Full baseline result follows after terminal execution.

PR #40's pre-review head 8fa7a63 has terminal successful Linux CI
[36634950074](https://github.com/bobby/suss/actions/runs/36634950074), verified by
the coordinating agent. That result does not certify the new review fixes; the
new pushed head requires its own successful run before the PR is ready.

Final full baseline: `CARGO_BUILD_JOBS=2 cargo test --workspace --locked -- --test-threads=2`
passed (exit 0), using the shared CARGO_TARGET_DIR. Full log:
/private/tmp/suss-review-pr40-full.log. Results: 14 CLI, 54 compiler, 317
expressions (12 existing ignored), 29 component, 9 strict conformance (2 manual
recorders ignored), 4 oracle (1 manual capture ignored), 3 shared-GC, 8 async,
8 toolchain profile, 8 core and 19 reader passes. Two existing doc examples remain
ignored. Differential corpus remains 9 passing/7 exact failures/0 skipped; legacy
201-case baseline remains zero failures/skips. `git diff --check` passed.

Review identified no additional significant findings within this PR's current
scope. Numeric/UTF-16/ExceptionInfo and full arity coverage, production shared
runtime integration, canonical allocation/resource lifetime management, broader
browser coverage and compiled persistence remain the documented roadmap work.
Next unblocked step: incorporate this base review fix into stacked PR #41, finish
that PR's dispatched review audit, and observe CI at both final heads. Continue
M2 reader/IR/shared-runtime integration afterward; do not close future milestones.

## Dispatched PR #41 code review — 2026-09-29

An independent dispatched reviewer inspected all six files in the runtime ABI
increment at 95a5d90 against the accepted design, roadmap and compatibility
inventory. The audit covered recursive type canonicalization, packed UTF-16 and
binary64 operations, closure callbacks and arity bounds, binding roots across GC,
exception tagging, manifest parsing and actual prelude verification before
initializer effects. Tests execute generated modules and independently inspect
their fields and units. No additional significant defect was found within this
explicitly bounded foundation increment; no artificial refactor was introduced.

The reviewer incorporated PR #40's independently reproduced fixes at fb3ec0a:
canonical WIT catch/finally local offsets and strict conformance trap-stage
classification. The branch integration preserves both handoff evidence sections;
this does not merge either pull request. The inherited code diff was inspected
and both new executing regressions pass on the combined runtime branch.

Commands/results, with CARGO_BUILD_JOBS=2 and the shared CARGO_TARGET_DIR:

- `cargo test -p suss-compile --test runtime_abi --locked -- --test-threads=2`:
  7 passed, 0 ignored, on the original PR head.
- `cargo test --workspace --locked -- --test-threads=2`: passed at 95a5d90;
  log /private/tmp/suss-pr41-review-full.log. All enabled suites passed, including
  317 expressions, 28 components, 8 conformance, 4 oracle and 7 runtime ABI tests.
  Existing ignores remain explicit; no expected results were changed.
- After integrating fb3ec0a,
  `cargo test -p suss-compile --test component --test conformance --test runtime_abi --locked -- --test-threads=2`:
  29 component, 9 conformance (2 manual recorders ignored), and 7 runtime ABI
  tests passed; log /private/tmp/suss-pr41-review-integration.log. No RUSTFLAGS
  override and no new semantic skips.

This review does not establish reader/HIR/IR/backend migration, source-level
immutable strings and checked dynamic operations, complete multi-arity dispatch,
unbound-var behavior, nominal protocols, ExceptionInfo or persistent REPL support.
Those remain documented integration/roadmap work. Source differential evidence
is still 9 passing, 7 exact known failures and 0 skipped; M2-03 is incomplete.

The user's ongoing rule is to open PRs, dispatch an independent review agent for
each, push fixes for significant findings, and require successful CI at each
final head before readiness. Do not merge PRs yet. CI for the combined pushed
head must be observed separately; local passes and earlier-head CI do not certify
it. Next unblocked task remains the reader/IR binary64 and UTF-16 migration into
the shared runtime, with explicit operand order and loader gating.

## M0/M1 issue reconciliation after merged PR #40 — 2026-09-29

The user merged #40/#41. PR #40 used related-issue references instead of closing
links, so completed acceptance work left issues #1–#7 open. Reconciliation now
maps each unchanged criterion to actual evidence in
[acceptance-m0-m1.md](acceptance-m0-m1.md). Local package statuses are completed
for bounded M0 feasibility and M1 evidence harnesses. This supersedes historical
notes that review or Linux CI still awaits PR #40 publication. Known semantic
failures remain failures; no production compiler, REPL, WASI or browser milestone
is inferred complete from a fixture.

Fresh checks use merged main fad9ee929224cec46265fb92b1acd3c9db82bab1:

- Two independent inventory generations match tracked SHA256 b6f3bce5e2847efd0eabee1b61c83914be1c659f41c6f51b04fdc4bcb8509f8c; 1,065 declarations remain unassessed. Review policy and 15-file/six-package WIT lock pass.
- 42 Python tests and offline roadmap preview pass.
- Focused toolchain_profile/toolchain_async/shared_runtime/conformance: 8/8/3/9 pass, two manual conformance ignores; /private/tmp/suss-acceptance-focused.log.
- Fresh scripts/test-oracle.sh passes: 16 reference observations, 9 differential passes/7 exact failures/0 skips; four Rust passes/one manual ignore. /private/tmp/suss-acceptance-oracle.log. No expectations changed.
- Fresh Chrome/Jco semantic and packaging fixture passes; /private/tmp/suss-acceptance-browser.json matches committed evidence exactly, including all 11 source and three generated hashes. Browser teardown still requires termination after complete DOM output; actual exit remains null.
- CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target cargo test --workspace --locked -- --test-threads=2: full baseline passed, exit 0; /private/tmp/suss-acceptance-full.log. No RUSTFLAGS changes, blanket skips, new dependencies or runtime modifications.
- Reviewed PR #40 CI36636314840 on fb3ec0a and merged-main CI36639533936 on fad9ee9 remain terminal success. The reconciliation PR requires its own dispatched review and final-head CI.

The reconciliation PR will use Closes #1 through Closes #7; remote issues remain
open until its user merge. M0/M1 milestone state remains open until linked issues
close and exit gates are rechecked. #10 (ABI foundation from #41) and #8 (reader
foundation from #42) remain incomplete; use Refs links for partial work.
AGENTS.md and the PR template now require full-acceptance closing links, partial
reference links, a dispatched reviewer with significant fixes pushed, final-head
CI and no agent merges.

Next unblocked implementation remains portable-form HIR with binding identity
and source spans, explicit evaluation-order/control-flow IR and shared ABI
lowering. Keep the lossless forms separate from legacy EDN; retire old paths
only after replacement acceptance. M2–M9 remain unfinished.

## Dispatched PR #43 acceptance review — 2026-09-29

An independent dispatched reviewer audited the unchanged M0/M1 criteria against
executing probe source, strict decoder/comparator regressions, pinned source and
license policy, fresh acceptance logs and Chrome/Jco evidence. GitHub confirms
successful CI at PR #40's reviewed fb3ec0a head and merged-main fad9ee9. The fresh
browser JSON exactly matches committed evidence, including all 11 source hashes;
its unknown process exit and teardown termination remain explicit.

M1-02 requires differential cases for the named boundaries and a development-only
oracle. Its 16 shared cases and strict 9-pass/7-exact-failure/0-skip baseline fulfill
that evidence requirement; they do not fulfill future compatibility acceptance.
Closing #1–#7 through PR #43 is justified by the unchanged bounded criteria. No
M2–M9 issue, semantic failure or unassessed declaration is marked complete.

The review corrected contradictory current prose in the oracle and compatibility
READMEs and outdated M1-02 issue evidence that still described the differential
oracle/transport as unimplemented. Historical observations remain intact.
Policy/consistency checks: 42 Python tests, inventory/review/WIT lock verification,
offline roadmap preview and diff whitespace check pass. Runtime code and expected
results are unchanged; the already completed exact full baseline remains relevant.
This review fix must be pushed and its own final-head CI observed before readiness.
No PR was merged and no issue or milestone was directly closed.

Next unblocked work remains lossless reader forms into HIR/binding identity,
explicit source-order IR and shared runtime lowering, as described above.

## Portable reader forms — 2026-09-29

Prior reviewed PR heads have terminal successful CI: #40 fb3ec0a in
[36636314840](https://github.com/bobby/suss/actions/runs/36636314840), and #41
2b4d2fe in [36636495795](https://github.com/bobby/suss/actions/runs/36636495795).
Both were verified open/draft and unmerged when those runs completed. A later
remote check shows both were externally merged into main, whose current head
is fad9ee9; their source branches were deleted. This agent did not merge them.
Each had the required dispatched review; #40's two significant findings were
pushed and inherited by #41. No milestone
was closed and M2–M9 remain incomplete.

Next increment, on resurrection/portable-reader-forms based on the now-updated
main (including #41), introduces
`suss_reader::forms`. Source forms retain byte spans, ordered metadata, binary64
numbers and UTF-16 strings separately from prototype EDN runtime values. Character
literals are one-unit strings; raw astral strings and escaped lone surrogates
preserve exact units. Ordinary integer literals round to binary64 rather than
creating an arbitrary-precision runtime value. Ratios/precision suffixes produce
explicit diagnostics. Numeric spelling support remains bounded, not a claim of
all upstream reader forms.

Conditional clauses are preserved in source order; resolution chooses the first
portable :suss/:cljs/:default clause, removes unmatched syntax and only then
pairs map entries. Quote/deref prefixes get source spans. Comments/discard and
unsupported/malformed input have explicit results. Metadata remains syntax,
not runtime metadata or an extra function argument. See docs/runtime/reader-forms.md.

Failure/repair evidence:

- Initial lone-surrogate regression through the old EDN reader failed with a
  parse error. The new reader preserves [0xd800, 0, 0xd83d, 0xde00] and passes.
- Initial 256-depth bound did not prevent actual test-thread stack overflow
  (SIGABRT). A 64-form bound now rejects excessive nesting with a located error;
  the regression passes. No blanket skip or increased test thread stack.
- Initial development Node runner failed resolving a corpus path relative to
  generated code. It now reads from the script's fixed oracle working directory;
  a fresh successful compile/execute/compare follows, not a fabricated exit code.

Validation (all Cargo commands use CARGO_BUILD_JOBS=2; no RUSTFLAGS):

- `cargo test -p suss-reader --locked -- --test-threads=2`: 19 existing parser
  tests plus 7 new portable form tests passed, 0 ignored.
- `scripts/test-reader-oracle.sh`: freshly compiled pinned ClojureScript with its
  tools.reader 1.3.6 dependency and Node; 14 original scalar observations match
  float bits/UTF-16 units exactly. JVM/Node remain development-only. The adapter
  and Rust implementation are original; no upstream core/reader source copied.
- `cargo test -p suss-compile --test reader_runtime --locked -- --test-threads=2`:
  2 passed, 0 ignored. Shared scalar corpus check plus actual generated ABI
  runtime transfer, independently inspected GC fields/units and forced GC.
- `cargo test --workspace --locked -- --test-threads=2`: exit 0;
  /private/tmp/suss-portable-reader-full.log. 317 expressions/12 existing ignored,
  29 components, 9 conformance/2 manual ignored, 4 oracle/1 manual ignored,
  2 reader-runtime, 7 ABI, 3 shared, 8 async, 8 profile, 8 core, 19 old reader and
  7 new reader passes. Two existing doctest examples remain ignored.
- Python regression suite: 42 passed. Formatting, `git diff --check`, and offline
  roadmap preview (10 milestones/39 stable issues) passed.

M2-01 is in-progress, not complete. Legacy AOT/REPL/compiler/macro/component reader
paths have not migrated to the portable forms. The 14 reader passes are not new
compiler compatibility passes: full source differential corpus remains 9 passing,
7 exact known failures and 0 skipped. Namespace file ambiguity, aliases/refers,
phase imports, cljs.core binding aliases, complete reader/syntax-quote behavior
and splicing conditionals remain work. No known-failure files were changed.

Next unblocked task: make HIR/explicit evaluation-order IR consume portable forms
without EDN conversion, lower values through shared ABI intrinsics, and wire the
manifest/layout gate into fragment loading. Migrate AOT/REPL/macros through the
same pipeline and retire prototype parser/runtime paths after acceptance. This
increment requires its own dispatched reviewer, fixes pushed for significant
findings and successful final-head CI; do not merge any PRs yet.

## Dispatched PR #42 code review — 2026-09-29

The independently dispatched reviewer audited the full 13-file increment at
81bc3c1 against the accepted design, roadmap, inventory and handoff. Review
covered scalar binary64/UTF-16 parsing, metadata/source spans, conditional
selection and map pairing, quote/discard/character syntax, malformed input and
stack bounds, the development-only pinned oracle, actual GC runtime transfers
and the explicit legacy-pipeline integration limitations.

One significant defect was reproduced and fixed: apostrophes were incorrectly
token terminators. Valid portable names such as `form'`, `ns/form'` and `:name'`
were split into quote syntax or failed with an unexpected end-of-input error.
Direct execution of the freshly compiled pinned tools.reader confirms these
names stay single tokens; it also rejects `\\'x` as an unsupported character
token while accepting `\\'`. The regression FAILED before the repair (exit 101)
and passes after it. Apostrophe now continues tokens but starts quote syntax at
a form boundary. Octal string escapes retain their separate reader-macro
termination rule, so `"\\1'"` remains units [1, 39]. No upstream source was copied.

Validation, with CARGO_BUILD_JOBS=2 and the existing shared CARGO_TARGET_DIR:

- `cargo test -p suss-reader --locked apostrophes_continue_tokens_but_quote_at_form_start -- --exact --test-threads=2`: red before the fix.
- `cargo test -p suss-reader --locked -- --test-threads=2`: 19 legacy and 8
  portable reader tests passed, 0 ignored, including the new regression.
- `cargo test --workspace --locked -- --test-threads=2`: passed, exit 0;
  /private/tmp/suss-review-pr42-full.log. All enabled suites pass, including
  317 expressions, 29 components, 9 conformance, 4 oracle, 2 reader-runtime,
  7 ABI, 3 shared-GC, 8 async, 8 profile, 8 core and 27 reader tests. Existing
  12 expression, 2 conformance, 1 oracle and 2 doc-example ignores remain.
- `python3 -m unittest discover -s scripts -p 'test_*.py'`: 42 passed.
- `rustfmt --edition 2024` on the touched Rust files and `git diff --check`:
  passed. No RUSTFLAGS override, new dependencies or known-failure changes.

No additional significant finding was identified within this bounded reader
foundation. Compiler/evaluator/component paths still use legacy EDN; namespace
phases, full syntax-quote/splicing and production HIR/IR/backend integration
remain open. The scalar corpus remains 14 reader observations, separate from
9 passing/7 exact failing/0 skipped compiler observations; M2-01 is incomplete.
Next unblocked task remains portable-form HIR/evaluation-order IR and shared-ABI
lowering/loader integration. Push this review fix to PR #42 and require successful
CI at that final head before readiness. Do not merge PRs.

## PR #42 follow-up prefix/conditional review — 2026-09-29

The coordinating agent reopened the dispatched review after spotting premature
metadata target validation. Pinned tools.reader execution with :read-cond :allow
and features :suss/:cljs confirms `^:export #?(:suss f :cljs g)` yields f with
export metadata. A selected scalar is invalid; an unmatched conditional lets
metadata seek the next retained target. The previous reader rejected even the
valid selected-symbol case before selection. Its focused regression FAILED at
94b0d19 (exit 101) and passes after this repair.

The same finding affects quote, deref, var-quote and discard: an unmatched target
conditional must disappear before a prefix finds its logical target. The old
quote wrapper produced a malformed one-item quote list plus an unwrapped next
form; its focused regression FAILED at 94b0d19 (two forms instead of one).
Raw syntax now retains Prefix(operator,target) and conditional-dependent
Discard(target) forms. Selection consumes retained targets in source order,
lowers prefixes to lists, and validates metadata only on the retained target.
Missing targets and scalar metadata targets produce located diagnostics.

Conditional feature/body syntax must also remain flat until prefix/discard
selection establishes its logical pairs. The deferred-discard clause regression
FAILED at 94b0d19 (premature odd-pair diagnostic) and passes after the repair.
Pinned Node execution confirms both ordinary and conditional-dependent discard
clause examples yield 2. Unselected branch bodies suppress nested feature
selection: `#?(:jvm #?(:jvm 1) :suss 2)` yields 2. Prefix target lookup stays
within its containing sequence or enclosing conditional; it cannot consume a
target outside a closing delimiter. Metadata order, operator byte spans,
multi-discard chains, map pairing and conditional-body boundaries are tested.
Both raw and synthesized nesting are bounded to 64, including many flat raw
prefixes that would otherwise build an unbounded resolved chain. No test stack
increase or blanket skip was added. No upstream implementation was copied.

The predecessor head 94b0d19 had terminal successful CI
[36641499909](https://github.com/bobby/suss/actions/runs/36641499909), verified by
the coordinating agent. This does not certify the follow-up fix. New-head CI
must pass after publication; no PR was merged by either agent.

Focused validation (CARGO_BUILD_JOBS=2, shared CARGO_TARGET_DIR, no RUSTFLAGS):

- `cargo test -p suss-reader --locked metadata_targets_are_checked_after_conditional_selection -- --exact --test-threads=2`: red at 94b0d19.
- `cargo test -p suss-reader --locked reader_prefixes_continue_after_unmatched_conditionals_with_bounded_depth -- --exact --test-threads=2`: red at 94b0d19.
- `cargo test -p suss-reader --locked conditional_clause_pairing_follows_deferred_prefix_selection -- --exact --test-threads=2`: red at 94b0d19.
- `cargo test -p suss-reader --locked -- --test-threads=2`: 19 legacy plus
  11 portable tests passed, 0 ignored, including all three repaired regressions.
- `python3 -m unittest discover -s scripts -p 'test_*.py'`: 42 passed.
- Formatting and `git diff --check`: passed. Final full baseline follows.

Final full baseline: `cargo test --workspace --locked -- --test-threads=2`
with CARGO_BUILD_JOBS=2 and the shared CARGO_TARGET_DIR passed (exit 0).
Log: /private/tmp/suss-review-pr42-prefix-verified-full.log. Results: 14 CLI,
54 compiler, 317 expressions/12 existing ignored (54.32s), 29 components,
9 conformance/2 manual ignored (22.13s), 4 oracle/1 manual ignored,
2 reader-runtime, 7 ABI, 3 shared-GC, 8 async, 8 profile, 8 core,
19 legacy reader and 11 portable reader passes. Two existing doc examples
remain ignored. Differential corpus remains 9 passing/7 exact failures/0 skipped;
no known-failure or expected scalar observations changed.

The syntax contract now documents flat conditional forms, pending prefixes and
deferred discards. Legacy compiler/evaluator/component paths remain unchanged;
this is not a new compiler compatibility claim or completed M2 acceptance.
Next task remains portable-form HIR/evaluation-order IR and shared runtime
lowering/loader integration. Keep the user-required review/fix/CI gates and do
not merge PRs.

## Portable HIR/IR to executing shared-ABI fragments — 2026-09-29

The coordinating agent replaced a temporary legacy delegation with the first
lossless source compiler path in `suss_compile::portable`. Reader selection feeds
HIR containing spans, ordered annotations and lexical binding identity, then
explicit typed blocks/values/edge parameters. Verification checks graph targets,
reachability, definitions/dominance, edge arities/types and numeric intrinsic
arity/types before emission. The backend cannot re-emit source operands: it only
reads value IDs. It emits the identical ABI group/manifest, imports used runtime
intrinsics and validates actual Wasm before returning an `eval -> Value` fragment.
No source form passes through EDN; no hidden print import, canonical memory, new
dependency or shipped Java/Node path was introduced.

The initial executing boundary regression FAILED with legacy delegation:
`missing runtime ABI manifest`, /private/tmp/suss-portable-pipeline-red.log.
After replacement, its generated artifact validates/links/executes and heap
inspection observes rounded binary64, not a legacy integer. An expanded semantic
regression then FAILED on `(let [x 1])` because the new frontend mistakenly
required a body; pinned macro source accepts optional body. Empty bodies now
return nil. Metadata remains separate syntax, and local callable shadowing
cannot fall through to a global arithmetic intrinsic.

Eleven focused tests execute/inspect fragments after GC, match the 14 scalar
reader cases, match a new 20-case compiled-source corpus, trace arithmetic imports
for once-only left-to-right behavior and short circuiting, inspect distinct
binding identities/annotations/diagnostics, reject malformed HIR/IR, execute a
backedge parameter swap to distinguish parallel from sequential assignment, and
retain earlier fragment values in one Store across later compilation/forced GC.
The source-level loop/recur compiler is not implemented by that IR-only swap.

Fresh source oracle: the first generated `.cljs` fixture FAILED with `Conditional
read not allowed`. The runner now generates `.cljc` for the #? case; fresh pinned
ClojureScript/Node observations match all 20 reviewed sources exactly, then Rust
executes the same source cases through actual generated ABI fragments. Five
Python regressions reject missing/duplicate/changed transport, invalid pin/schema,
boolean/integer confusion and extra/trailing data. The original 16-case legacy
corpus remains 9 differential passes/7 exact failures/0 skips; none of its expected
failures changed. All 1,065 inventory entries remain unassessed. No upstream core
implementation was copied; the bootstrap and original corpus have pinned source
provenance described in docs/runtime/portable-pipeline.md.

Commands/results (CARGO_BUILD_JOBS=2, no RUSTFLAGS override):

- `cargo test -p suss-compile --test portable_pipeline --locked -- --test-threads=2`: 11 passed, 0 ignored; /private/tmp/suss-portable-pipeline-final-focused.log.
- `scripts/test-portable-pipeline-oracle.sh`: fresh 20 source observations match exactly; all 11 executing/negative Rust tests pass. /private/tmp/suss-portable-pipeline-oracle.log. Initial .cljs harness failure is recorded above, not called success.
- `cargo test --workspace --locked -- --test-threads=2`: full baseline passed, exit 0; /private/tmp/suss-portable-pipeline-full.log. Existing 317 expressions/12 ignored, 29 components, 9 conformance/2 manual ignores, 4 oracle/1 manual ignore, 7 ABI, 3 shared-GC, 8 async, 8 profile, 8 core and 19+11 reader passes remain. New pipeline tests: 11 passes. Two existing doc examples remain ignored.
- `python3 -m unittest discover -s scripts -p 'test_*.py'`: 47 passed.
- Inventory check/review overlay/WIT lock/offline roadmap preview: passed, unchanged pin/hashes and 10 milestones/39 stable issues.
- Formatting on new Rust files and `git diff --check`: passed.

This is the replacement bootstrap, not production migration. CLI/AOT/components,
macro evaluator and source-replaying REPL still use the legacy pipeline. Namespace
files/aliases/refers/phases, full macro expansion, general closures/callees, live
binding cells, dynamic checking/coercion, collections/dispatch, source recur/tail
checks, throws/suspensions and target adapters remain. Arithmetic is restricted
to statically proven Number operands; dynamic operands fail with locations rather
than reaching unchecked casts. No inventory definition or M2 milestone is marked
complete. M2-03 machine status is reconciled from planned to in-progress to match
its already merged runtime foundation and new partial source lowering.

Open this increment stacked on reviewed PR #42 if it remains unmerged; use
Refs #8, Refs #9 and Refs #10, never closing links for incomplete packages.
Dispatch the required independent code-review agent, push significant fixes and
observe successful CI on the final head. Do not merge PRs. Next unblocked work:
namespace/phase/binding-cell resolution and universal closure/callee lowering with
dynamic checks, then collection/dispatch/recur/exception/async IR and unified
AOT/REPL/macro migration. Keep the portable source corpus as acceptance evidence;
retire obsolete backend paths only after the replacement passes their gates.


## Dispatched PR #44 code review — 2026-09-29

The independent reviewer audited the bounded portable compiler increment at
7fbfd02, including lexical resolution, spans/annotations and binding identities,
source order, numeric lowering, graph definitions/dominance/edge types, actual
shared-ABI emission/validation and strict oracle transport. Two significant
findings were reproduced with red regressions and repaired:

- `let` is a macro hidden by local bindings in pinned ClojureScript, unlike the
  actual `if`/`do` special forms. `(let [let 7] (let [] 1))` previously emitted
  a fragment returning 1. The located diagnostic regression FAILED at 7fbfd02
  (exit 101), `/private/tmp/suss-pr44-review-red.log`. Bootstrap resolution now
  reports the existing local-call closure-lowering diagnostic at the inner
  `let`, instead of silently invoking binding syntax. Executing positive
  regressions preserve true special forms and restore `let` after lexical scope.
  Fresh pinned ClojureScript/Node execution of the original review probe
  `(let [let (fn [& args] 42)] (let [] 1))` prints 42; the scalar-local probe
  throws TypeError. These are development-only reference observations, not
  claimed Suss closure support. No upstream implementation was copied.
- Public HIR directly representing `Arithmetic::Negate` silently returned its
  operand; zero-argument subtract/divide/negate silently produced an identity.
  The executing signed-zero regression FAILED at 7fbfd02: bits
  0000000000000000 instead of 8000000000000000 (exit 101),
  `/private/tmp/suss-pr44-review-negation-red.log`. Lowering now emits unary
  negation and rejects invalid Negate/Subtract/Divide arities with the HIR span.
  The repaired regression executes validated/linked Wasm and independently
  decodes negative-zero bits, then checks four malformed HIR arities.

Focused validation uses CARGO_BUILD_JOBS=2 and the shared CARGO_TARGET_DIR;
no RUSTFLAGS override, new dependency, expected-failure change or blanket skip:

- `cargo test -p suss-compile --test portable_pipeline --locked -- --test-threads=2`:
  13 passed, 0 ignored; `/private/tmp/suss-pr44-review-focused.log`.
- Fresh review reference: generated ignored `suss-oracle.pr44-review` fixture,
  `clojure -Srepro -M -m cljs.main` with Node target, then
  `node out/pr44-review.js`: exit 0, observations 42 and TypeError, using the
  initialized pinned submodule and existing development-only oracle deps.
- `python3 -m unittest discover -s scripts -p 'test_*.py'`: 47 passed.
- `rustfmt --edition 2024` on touched Rust files and `git diff --check`: passed.

Full baseline: `cargo test --workspace --locked -- --test-threads=2`, with
CARGO_BUILD_JOBS=2 and the shared CARGO_TARGET_DIR, passed (exit 0);
`/private/tmp/suss-pr44-review-full.log`. All enabled suites pass, including
14 CLI, 54 compiler, 317 expressions/12 existing ignores, 29 components,
9 conformance/2 manual ignores, 4 oracle/1 manual ignore, 13 pipeline,
2 reader-runtime, 7 ABI, 3 shared-GC, 8 async, 8 profile, 8 core and
19 legacy/11 portable reader tests. Two doc examples remain explicitly ignored.
Inventory regeneration and review overlay checks pass (0 reviewed/1,065
unassessed). Local green is ready for publication; final-head CI must be observed
by the coordinating agent after push.
The original 20-case bounded source corpus and 14 scalar observations remain
separate from the unchanged legacy 9 passes/7 exact failures/0 skips; all 1,065
inventory declarations remain unassessed. No M2 milestone, issue or PR is closed.
General closures/callees, namespace/phase/binding-cell resolution, dynamic checks,
collections/recur/exceptions/async and CLI/AOT/REPL/macros migration remain open.
Next unblocked task remains namespace/phase/binding-cell resolution and universal
closure lowering with dynamic checks. Push these fixes to PR #44 and observe
successful CI on their final head; initial-head CI does not certify repairs.
Do not merge PRs.

## Portable namespace identities and live-cell reads — 2026-09-29

The previous turn made concrete progress: dispatched PR #44 reviewer pushed two
significant fixes at 2ab729fe659ab39a7de12f33ace373f5f0c5747b; final-head CI
36650580806 completed successfully at 2026-09-30T00:41:05Z. PR #42/#43/#44 remain
open/unmerged as this increment starts; main is fad9ee9. No agent merged a PR.
Issue #9 records review/final-CI evidence; PR #43 carries closing links for #1–#7,
while partial M2 increments use Refs only. Keep the full roadmap goal active.

New branch resurrection/portable-resolution builds on reviewed PR #44. An
original qualified-global integration probe FAILED in the old portable frontend
(`app/value`, unsupported form, exit 101; /private/tmp/suss-resolution-red.log).
The final regression declares the var explicitly in an Environment and links its
runtime cell; an undeclared `app/value` still correctly fails. HIR/IR now retain
stable phase/namespace/name identities and emit ordered dynamic Value reads,
importing exact shared binding-cell types. No legacy EDN conversion or source
replay is involved. Canonical core aliases share one cell import. Namespace scopes
retain aliases/refers/exclusions independently by namespace and phase. Configuration
rejects ambiguity before mutation; source analysis cannot mutate the Environment.
The path resolver rejects ambiguous .sus/.cljs/.cljc sources across all roots,
deduplicates canonical files, and diagnoses invalid/missing namespace paths.

An executing unbound-cell regression FAILED because binding-get ignored the bound
flag and returned nil (exit 101, /private/tmp/suss-resolution-unbound-red.log).
The shared runtime now has binding-unbound, a checked getter raising tagged
Unbound binding and a setter marking cells bound, including nil. The test catches
the specific language tag in validated Wasm and independently inspects descriptor,
UTF-16 message and nil fields, then observes bound nil. No ABI layout changed.
A namespace re-entry regression FAILED when the initial Environment erased aliases
(exit 101, /private/tmp/suss-resolution-scope-red.log); namespace/phase scopes now
persist and dormant refers also reject conflicting declarations. A test fixture
borrow error and an incorrect metadata-span expectation were repaired; metadata
form spans include their reader prefix. These were not skipped or called success.

Bootstrap macro lookup remains bounded but separate from runtime vars, matching
pinned cljs/analyzer.cljc resolve-var/get-expander*: runtime vars do not hide core
let, lexical locals do, and referred/qualified macros retain identity after runtime
var redefinition. Arbitrary compiled macros are not implemented. Original Rust,
tests and the new qualified-core-let case copy no upstream implementation. Future
core ports still need the EPL/provenance process.

Final local evidence (CARGO_BUILD_JOBS=2; no RUSTFLAGS override):

- `cargo test -p suss-compile --test portable_resolution --test portable_pipeline --test runtime_abi --locked -- --test-threads=2`: 10 namespace/cell, 13 pipeline and 7 ABI tests pass, 0 ignored; /private/tmp/suss-resolution-final-focused.log.
- `scripts/test-portable-pipeline-oracle.sh`: fresh pinned ClojureScript/Node observations match all 21 source cases exactly, including qualified cljs.core/let; 13 executing pipeline tests pass. /private/tmp/suss-resolution-oracle.log. The original 14 scalar reader cases also still execute through compiled fragments.
- `cargo test --workspace --locked -- --test-threads=2`: final full baseline passed, exit 0; /private/tmp/suss-resolution-full.log. First full run passed too; after the final macro-reference fix, focused/full suites were rerun. Existing enabled suites pass and manual/legacy/doc ignores remain unchanged; new namespace/cell suite: 10 passes.
- `python3 -m unittest discover -s scripts -p 'test_*.py'`: 47 passed. Inventory check and review overlay: 1,065 declarations, 0 reviewed, 1,065 unassessed, unchanged pin/hashes.
- `python3 scripts/wasi_lock.py`: verifies 15 WIT files / 6 packages; /private/tmp/suss-resolution-wit.log. An attempted unsupported --check option exited 2 before checks; the correct command above passed.
- Offline `python3 scripts/publish_roadmap.py` preview: passed, 10 milestones/39 stable issues; /private/tmp/suss-resolution-roadmap-preview.json. No issues or milestones closed.
- rustfmt check on touched Rust and `git diff --check`: passed.

Actual fragments validate/link/execute, independently decoded numeric/exception
values survive GC, old code reads updated cells while old returned values remain
live, and a wrapping getter traces once-only source order/short circuiting. Missing
and wrong-type cell imports fail before eval. Mutable globals never inherit stale
Number facts into unchecked arithmetic. Separate phase identities execute distinct
cells; this is not an isolated compiled macro session. The legacy source corpus
remains 9 differential passes/7 exact failures/0 skips; no known failure changed.

Limits: source ns/require forms, module dependency loading/declared-ns validation,
privacy/cycles, source def/defonce, live callable values/universal closure calls,
dynamic checking, compiled macros, generic effects/throws/catch/suspensions,
collections/recur and CLI/AOT/REPL migration remain. The path resolver only locates
files. API callers supply phase-specific declarations and cells; a real linker
must share correct var identities and enforce dependency/cache/initialization
policy. Bootstrap numeric intrinsics are not a fully live core function library.
No M2 acceptance package is complete. See docs/runtime/portable-resolution.md.

Open this increment stacked on PR #44 while it remains unmerged, Refs #8/#9/#10.
Dispatch an independent PR reviewer to push significant fixes, then require CI
success at the final reviewed commit. Do not merge PRs. Next unblocked integration:
universal closures/callee/argument lowering and dynamic checks through resolved
live globals, followed by source namespace/definition loading and one production
pipeline for AOT/REPL/compiled macros. Retire obsolete paths only after acceptance.


## Source closures and ordered universal calls — 2026-09-30

PR #45's independently reviewed head 6e57ee12ebc99e3069b7ba575cfaaf139f56e813
passed final CI run 36653523291. It remains open and unmerged. This increment
builds on that head in resurrection/portable-closures; no agent merged any PR.

An original source-capture regression failed on the preceding frontend because
fn/local calls were unsupported (exit 101; /private/tmp/suss-closures-red.log).
The portable pipeline now lowers fixed anonymous fn/fn*, lexical captures and
computed/local/live-global calls into actual shared-ABI function references.
Captures use unique lexical identities and immutable value references; globals
remain live reads unless explicitly captured via a local. Nested closures capture
only required free values. Callee and arguments normalize once in source order.
Parameters retain source metadata without turning hints into unchecked type facts.
The verifier checks separate closure body entries, capture facts, calls and known
arities, bounding nesting and rejecting malformed huge arities before allocation.

The runtime checks callable/argument-array types before casts and central arity
before invocation, raising specific shared language exceptions rather than Wasm
cast traps. Executing wrappers prove argument effects occur even when the callee
is not callable. Cross-fragment tests preserve old captured function values after
rebinding/GC while live lookup sees the replacement. A test-only Wasm catcher
independently decodes actual tagged errors and reuses the Store after failure;
this does not claim production REPL recovery or source exception integration.

An emitter fixture initially failed compilation because a UTF-16 unit index
shadowed the SSA local offset; it was corrected. An ambiguous test float was also
corrected. No failing cases were skipped or replaced with success.

Validation (CARGO_BUILD_JOBS=2; no RUSTFLAGS override):

- `cargo test -p suss-compile --test portable_closures --test portable_pipeline --test portable_resolution --test runtime_abi --locked -- --test-threads=2`: 11 closure, 13 pipeline, 10 resolution and 7 ABI tests pass, zero ignored. Final run after the last diagnostic-text change: /private/tmp/suss-closures-publish-focused.log, exit 0.
- `scripts/test-portable-pipeline-oracle.sh`: all 34 original portable source cases match fresh pinned ClojureScript/Node observations exactly; 13 executing pipeline tests pass. /private/tmp/suss-closures-oracle.log, exit 0.
- `cargo test --workspace --locked -- --test-threads=2`: full baseline passes, exit 0; /private/tmp/suss-closures-full.log. This run preceded the final unsupported-core-value diagnostic-text/comment edits; the focused suite above covers those edits, and PR review will validate the published head. Existing manual/legacy/doc ignores remain unchanged.
- `python3 -m unittest discover -s scripts -p 'test_*.py'`: 47 pass. Inventory and review validation retain 1,065 declarations, zero reviewed and 1,065 unassessed; source pins/hashes unchanged.
- `python3 scripts/wasi_lock.py`: 15 WIT files / 6 packages verified. Offline roadmap publisher preview passes with 10 milestones / 39 stable issues; /private/tmp/suss-closures-roadmap-preview.json.
- rustfmt check on touched Rust and `git diff --check`: pass.

The original legacy corpus remains 9 differential passes / 7 exact failures /
0 skips; expected failures are unchanged. Original Rust and cases copy no upstream
implementation. Syntax/resolution was checked against pinned cljs/core.cljc and
cljs/analyzer.cljc at c4295f303100bbf5afac449242d30bca1126f1a1. Future core ports
still require EPL notices and extraction/patch hashes. No new dependency, shipped
Java/Node path, table or linear memory was introduced.

Limits: named/multi-arity/variadic/destructuring/pre-post signatures, dynamic
numeric checking/coercion and first-class bootstrap core values remain open.
Dynamic error payloads still lack source call-site annotations. Full IFn dispatch,
source exceptions/effects/recur/async lowering, source ns/module/definition loading,
compiled macros and CLI/AOT/REPL migration are not complete. See
[closure lowering](../runtime/portable-closures.md). M2 packages stay in progress.

Publish stacked on #45 with Refs #9 and Refs #10, never closing incomplete issues.
Dispatch an independent PR reviewer, require fixes for significant findings and
successful CI at the final reviewed commit. Review and CI are pending publication.
Next unblocked task: connect source namespace/module/definition loading to the
resolved shared cells and persistent compiled clients, then extend signatures,
checked core coercions and the remaining IR forms. Keep the full roadmap active;
retire obsolete paths only after replacement acceptance. Do not merge PRs.


## Dispatched PR #46 code review — 2026-09-30

The independent reviewer audited the entire closure increment at published head
5e59e616661827db669db69d13c49e4349c29db6 in an isolated worktree. Review covered
fn macro/lexical precedence, free binding identities and metadata, nested captures,
public HIR/IR entry/type/arity/dominance guards, universal function references and
local offsets, once-only callee-before-argument effects, cross-fragment roots/live
rebinding and actual tagged language errors. One significant semantic omission was
reproduced and fixed; no additional significant finding remained.

Pinned cljs/core.cljc's fn macro reads :pre/:post from parameter-vector metadata
when no explicit condition map exists. `((fn ^{:pre [false]} [] 1))` previously
compiled a validated fragment ignoring the precondition. The regression FAILED
at the published head, exit 101, /private/tmp/suss-pr46-review-red.log. Bootstrap
fn now rejects signature pre/post metadata with a located unsupported-feature
diagnostic. Regressions cover bare, qualified and renamed referred core fn,
shorthand metadata, ordinary annotations and actual fn* execution. Rejection is
conservative even for nil/false condition values or overwritten metadata keys;
pre/post support remains incomplete. The true fn* special form does not expand
these macro conditions and retains its previous behavior.

A fresh development-only pinned ClojureScript/Node fixture confirms pre and post
conditions reject the value, while fn* with the same metadata returns 1. Generated
ignored fixture: tests/oracle/out/generated/suss_oracle/pr46_review.cljs; build and
observations: /private/tmp/suss-pr46-review-reference.log. No upstream code was
copied and no shipped Java/Node dependency was introduced.

Commands/results (CARGO_BUILD_JOBS=2 and shared CARGO_TARGET_DIR; no RUSTFLAGS):

- `cargo test -p suss-compile --test portable_closures --test portable_pipeline --test portable_resolution --test runtime_abi --locked -- --test-threads=2`: published head 41 passes; repaired final focused suite 42 passes (12 closure/13 pipeline/10 resolution/7 ABI), zero ignored. Logs /private/tmp/suss-pr46-review-focused.log and /private/tmp/suss-pr46-review-final-focused.log.
- `cargo test --workspace --locked -- --test-threads=2`: published head and repaired implementation both pass, exit 0; /private/tmp/suss-pr46-review-full.log and /private/tmp/suss-pr46-review-final-full.log. The repaired full run began before the final renamed-refer assertion was added; the final focused run above covers that assertion. No implementation changed afterward.
- Pinned reference: `clojure -Srepro -M -m cljs.main` with Node target compiling suss-oracle.pr46-review, then `node out/pr46-review.js`: exit 0; observations pre rejected, post rejected, 1.
- rustfmt on touched Rust and `git diff --check`: pass.

Enabled baseline suites remain green; existing manual/legacy/doc ignores and the
legacy oracle's 9 differential passes/7 exact failures/0 skips are unchanged.
The original 34-source portable corpus remains passing; all 1,065 inventory
entries remain unassessed. Extended signatures, pre/post execution, dynamic numeric
coercions/core values, source namespace/module/definition loading, full IFn,
exceptions/effects/recur/async, compiled macros and production client migration
remain incomplete. No M2 package or broad issue is closed. Next unblocked work:
source namespace/module/definition loading connected to resolved shared cells and
persistent compiled clients. Push the review repair to PR #46 and require
successful CI on the repaired head before readiness. Do not merge PRs.


## Open PR stack conflict repair — 2026-09-30

At the user's request, rebased #42 -> #44 -> #45 -> #46 onto merged main
a9a910d in /private/tmp/suss-stack-rebase. The only conflict was concurrent
append-only handoff evidence from #43 and #42; both sections are retained.
Dependent commits replayed cleanly. Runtime code, test cases, expected failures,
source pins, issue IDs and incomplete M2 statuses are unchanged. Merged M0/M1
acceptance and policy changes now survive throughout the stack. An independent
subagent reviewed each of the four rebased PRs and found no significant issue.
The original portable-definitions checkout and its unfinished work are untouched.

Validation (CARGO_BUILD_JOBS=2, existing shared CARGO_TARGET_DIR; no RUSTFLAGS):

- `cargo test -p suss-reader -p suss-compile --test forms --test reader_runtime --test portable_pipeline --test portable_resolution --test portable_closures --test runtime_abi --locked -- --test-threads=2`: 55 passed, zero ignored; /private/tmp/suss-stack-focused.log.
- `python3 -m unittest discover -s scripts -p 'test_*.py'`: 47 passed.
- `python3 scripts/cljs_inventory.py --check` and `python3 scripts/cljs_reviews.py`: pinned 1,065 declarations verified, zero reviewed / 1,065 unassessed. Initial checks failed because the isolated worktree lacked an initialized reference checkout; a local worktree at the exact pinned ClojureScript revision repaired setup, then both checks passed. No pin or expected result changed.
- `python3 scripts/wasi_lock.py`: 15 files / six packages verified.
- `python3 scripts/publish_roadmap.py`: offline preview passed; /private/tmp/suss-stack-roadmap.json.
- Per-PR `git range-diff` and `git diff --check`: pass; original implementation patches retained.
- `cargo test --workspace --locked -- --test-threads=2`: passed, exit 0; /private/tmp/suss-stack-full.log. Existing explicit manual/legacy/doc ignores are unchanged. Final-head GitHub CI remains pending and must be observed separately.

Publish all four rewritten heads atomically with explicit old-head leases. No PR
is merged by this repair. Next unblocked implementation remains source namespace,
module and definition loading through resolved shared cells and persistent
compiled clients; the unfinished portable-definitions work is not part of this
repair. Full baseline and final-head CI results must be reported after completion.
## Source definition and namespace preparation — 2026-09-30

PR #46's dispatched reviewer pushed significant metadata-condition fix 7b42b4d;
final CI 36657734298 passed on 7b42b4d2e87997238ad25dc53f9f035ed286814d. It remains
open/draft and unmerged. This increment originally built on that head in
resurrection/portable-definitions. During publication, the user-authorized stack
repair rebased the parent onto merged #43; this increment was then replayed onto
d77f29edc7ccbf407485513211ffc51ee9d74fb9, retaining both appended handoff sections
and the merged M0/M1 acceptance/policy changes. Runtime code was unchanged by
that reconciliation. No agent merged PRs or closed incomplete issues.

An executing source `(def value 7) value` regression FAILED on the old frontend:
unresolved def, exit 101, /private/tmp/suss-definitions-red.log. Source def and
bounded bootstrap defonce now register stable identities in a private compiler
snapshot and lower to explicit GlobalBound/GlobalWrite IR. Initializers evaluate
once before cell publication; a failure preserves the previous value while
preceding completed effects remain. Bound nil/false skip defonce initialization.
Declarations preserve existing cells and leave new ones unbound. Def is a true
special form; defonce retains core macro identity/lexical shadowing rules.
Name metadata/spans and optional UTF-16 docstrings remain in HIR. Unsupported
const/dynamic/private/macro/export definition attributes are located errors.

prepare_fragment returns validated Wasm, staged Environment, current-phase cell
identities and retained leading ns directive. Source callers share one pipeline.
Leading ns supports bounded require aliases/refers/renames and core exclusions/
renames against supplied declarations, not a recursive file loader. Failed source
compilation cannot mutate the caller environment or allocate host bindings. Tests
reuse one Store and cells without replay, but are not a production session API.
An ns scope reset regression FAILED because an old + exclusion survived a fresh
ns declaration (exit 101, /private/tmp/suss-definitions-ns-reset-red.log). Source ns
now replaces that phase's imports/exclusions while preserving cells; API namespace
re-entry still preserves scopes. Qualified cljs.core definitions initially failed
as a different namespace (exit 101, /private/tmp/suss-definitions-core-alias-red.log);
they now select the canonical suss.core cell and the regression executes.

Fresh pinned ClojureScript execution confirms def value returns, declaration
statements preserving existing values, nil/false defonce and lexical initializers.
An initial oracle fixture put initializerless def directly in a println argument;
the pinned compiler emitted invalid JavaScript and Node failed. A corrected
statement-context fixture executes, exit 0; /private/tmp/suss-definitions-reference-corrected.log.
The failed fixture is retained as a limitation, not called successful. Test fixture
float annotations and a metadata-span expectation were also corrected; reader
spans include the metadata prefix. No semantic failure was skipped.

Validation (CARGO_BUILD_JOBS=2; no RUSTFLAGS override):

- `cargo test -p suss-compile --test portable_definitions --test portable_closures --test portable_pipeline --test portable_resolution --test runtime_abi --locked -- --test-threads=2`: final 52 passes, zero ignored (10 definitions/12 closures/13 pipeline/10 resolution/7 ABI); /private/tmp/suss-definitions-final-52-focused.log, exit 0.
- `scripts/test-portable-pipeline-oracle.sh`: fresh pinned ClojureScript/Node observations match all 42 original portable source cases; 13 executing pipeline tests pass. /private/tmp/suss-definitions-oracle.log, exit 0. The last core namespace fix affects a separate executing regression, not these cases.
- `cargo test --workspace --locked -- --test-threads=2`: preceding implementation baseline and ns-reset repaired baseline pass, exit 0; /private/tmp/suss-definitions-full.log and /private/tmp/suss-definitions-final-full.log. Final publication baseline on the frozen core-alias repair also passed, exit 0; /private/tmp/suss-definitions-publish-full.log.
- `python3 -m unittest discover -s scripts -p 'test_*.py'`: 47 pass; /private/tmp/suss-definitions-python.log. Inventory/hash checks retain 1,065 declarations, zero reviewed / 1,065 unassessed.
- `python3 scripts/wasi_lock.py`: 15 WIT files / 6 official packages verified. Final offline roadmap preview passes with 10 milestones / 39 stable issues; /private/tmp/suss-definitions-final-roadmap-preview.json.
- rustfmt check on touched Rust and `git diff --check`: pass.

Actual artifacts validate/link/execute across fragments, retain captures and live
lookups after rebinding/GC, and preserve initializer publication/effect order.
Test-only Wasm catchers catch the specific shared language tag, independently
inspecting descriptor IDs, UTF-16 messages and nil payloads; traps cannot pass.
Malformed public bound/write IR is rejected before emission. Production ABI
recursive layouts are unchanged; binding-bound is a new private intrinsic.

Implementation/tests are original; pinned cljs/core.cljc defonce and
cljs/analyzer.cljc def/ns, licensed EPL-1.0, informed semantics. No implementation
was copied; future upstream ports still require notices/extraction/patch hashes.
No new dependency or shipped JVM/Node path exists. The original legacy corpus
remains 9 differential passes / 7 exact failures / 0 skips; inventory items and
expected failures are unchanged. See docs/runtime/portable-definitions.md.

Limits: recursive source dependency/file loading, declared namespace validation,
cycle/privacy/reload/initialization policy, source require-macros and isolated
compiled macro sessions, runtime metadata/attributes, persistent production
CLI/AOT/REPL clients, extended signatures/core coercions/collections and remaining
exception/effect/recur/async IR are unfinished. Initializerless def used as a value
is not separately certified against an executable pinned artifact. M2/M3 acceptance
stays incomplete. Publish stacked on #46 with Refs #8/#9/#10/#13, dispatch an
independent reviewer to push significant repairs, and require final-head CI. Review
and CI are pending publication. Do not merge.

Next unblocked task: recursive source namespace/module loading connected to staged
compiler declarations and persistent compiled clients, including deterministic
missing/mismatched/ambiguous/cyclic errors and initializer policy. Keep the full
roadmap active; retire obsolete paths only after replacement acceptance.


## Dispatched PR #48 review — 2026-09-30

The independent reviewer audited source definition/namespace preparation, staged
compiler state, special/macro precedence, metadata and qualification, nested
captures, bound/write dominance and imports, initializer publication/failure
ordering and actual shared-tag errors/GC. Published de2197f passed the 52-test
focused suite and full workspace baseline before the stack reconciliation. Review
resumed on reconciled 6d199ab77e2082e7e60f67fafc89e26f3bb8ba2b over parent
d77f29edc7ccbf407485513211ffc51ee9d74fb9; the reconciliation changed no runtime
code. Two significant boundary omissions reproduced and were repaired.

Source require libspec ^:reload metadata previously compiled while silently
losing initialization policy. Pinned cljs/analyzer.cljc explicitly reads that
metadata. It now produces a located unsupported diagnostic until source loading
and reload policy exist. Rejection is conservative for false/overwritten reload
keys; ordinary retained metadata remains accepted and executes.

Initializerless def previously invented a nil result in arbitrary expression
contexts even though the pinned direct-argument fixture emitted invalid JS.
It now fails with a located unsupported diagnostic in initializer, call operand,
condition and closure-result contexts. Analysis propagates statement context
through do/let/if; top-level and intermediate declaration statements still execute
and preserve existing cells. Initializer-bearing def/defonce behavior is unchanged.
The new regressions failed first (10 passing/2 failing, exit 101;
/private/tmp/suss-pr48-review-red.log); neither failure was skipped.

Commands and terminal results (CARGO_BUILD_JOBS=2, shared CARGO_TARGET_DIR, no
RUSTFLAGS override):

- `cargo test -p suss-compile --test portable_definitions --test portable_closures --test portable_pipeline --test portable_resolution --test runtime_abi --locked -- --test-threads=2`: final 54 passes (12 definitions/12 closures/13 pipeline/10 resolution/7 ABI), zero ignored, exit 0; /private/tmp/suss-pr48-review-final-focused.log.
- `cargo test --workspace --locked -- --test-threads=2`: published and repaired full baselines passed, exit 0; /private/tmp/suss-pr48-review-full.log and /private/tmp/suss-pr48-review-final-full.log. Final implementation changed only by an explanatory comment after focused compilation; the full baseline covers that comment. Existing manual/legacy/doc ignores remain unchanged.
- `python3 -m unittest discover -s scripts -p 'test_*.py'`: 47 passes, exit 0; /private/tmp/suss-pr48-review-python.log.
- Offline `python3 scripts/publish_roadmap.py`: passed, exit 0; /private/tmp/suss-pr48-review-roadmap-preview.json. Stable issue IDs retained.
- Touched-file rustfmt check and `git diff --check`: passed. An initial workspace-wide format invocation touched legacy files; every unrelated formatting edit was restored before the final full run.

Original source cases remain 42; fresh pinned oracle execution on the repaired
published head and final-head CI are pending root verification. Legacy differential
results remain 9 passes/7 exact failures/0 skips, and all 1,065 inventory declarations
remain unassessed. Original review code/tests copy no upstream implementation;
pinned EPL-1.0 analyzer informed the reload restriction. No shipped Java/Node
path, new dependency or runtime ABI layout change was introduced.

Merged #43 main a9a910d passed CI 36659402729; root independently verified its
acceptance evidence and closed remote M0/M1 milestones after issues #1–#7 were
already closed. Repository M0/M1 completion remains intact; M2–M9 remain open.
The reviewer and root did not merge any PR during this review.

No additional significant finding remained. Push the repair to PR #48 and require
successful CI at the final reviewed commit. Recursive source loading, declared-ns
validation, cycles/privacy/reload policy, source macro imports, runtime attributes,
compiled macros, persistent production CLI/AOT/REPL clients, dynamic core coercions,
extended signatures/collections and remaining exception/effect/recur/async IR
remain unfinished. Next unblocked task: recursive source namespace/module loading
connected to staged declarations and persistent compiled clients, with deterministic
dependency errors and explicit initializer policy. Do not merge PRs or close
incomplete M2/M3 packages.

## Proposed Result/Option/panic semantic direction — 2026-09-29

Added [ADR-0001](../adr/0001-result-option-and-panic.md) at the user's request,
with an ADR index and a proposal-only link from the accepted design. It describes
complete native adoption of nominal Result/Option/Unit, optional Error diagnostic
protocol, recoverable values and panic in place of application throw/catch, and
separate replacements for nil's absence/completion/exhaustion/null roles. It also
describes tactical nominal WIT bindings alongside unchanged ClojureScript
exception/nil semantics, including Some(nil), explicit lossy conversions, and
language-exception adapters that cannot swallow traps or panic.

Both stages remain proposed. The accepted semantics, runtime ABI, implementation,
inventory classifications, known failures, roadmap issue IDs and milestone
acceptance statuses were not changed. Examples and test gates are design targets,
not executing features or passing evidence. Panic cleanup/supervision, detailed
core migration, profile/linking policy and final API syntax remain open decisions.
No PR, publication, merge, upstream core port or shipped dependency was introduced.

Validation: `git diff --check` passed. A Python pathlib/re check verified balanced
code fences in the two new ADR files and accepted-design document, and all 11
local Markdown link targets in those files exist. Primary WIT/Rust documentation
was checked for options, results, payloadless cases and Result/panic distinctions;
the ADR links those references. Rust tests were not run for this documentation-only
proposal; no compiler/runtime/test behavior changed.

Next unblocked ADR task: review and decide whether to accept the tactical boundary
change and/or strategic goal, then specify nominal constructors, matching and
independent cross-fragment/WIT acceptance fixtures before implementation. Existing
production-session roadmap work remains unblocked by this proposal.

## Isolated ADR-0001 publication and work tracking — 2026-09-29

At the user's request, copied only the ADR documentation into the separate
/private/tmp/suss-adr0001-worktree worktree on proposal/result-option-panic,
based on origin/main 8686437. The active portable-session worktree and its
implementation files were not modified by publication work.

Created dedicated GitHub milestone 11 and eight open work packages ADR1-01 through
ADR1-08 (issues #50–#57). docs/adr/0001-tracking.json records their remote links
and dependencies; the ADR index and ROADMAP link this separate proposal track.
Existing M0–M9 IDs, acceptance states, inventory and known failures are unchanged.
Decision/design gates precede implementation, and native migration requires
explicit strategic acceptance. No issue or milestone was closed.

The dispatched independent review approved the final documentation/tracking and
all eight live issue bodies with no significant findings or edits. Final-head
PR CI remains pending publication; this entry does not claim its result. `git diff --check` passed. The focused
Python documentation check passed for four Markdown files, 22 local link targets,
balanced fences, and eight unique/topologically ordered tracking entries matching
the index links. GitHub API verification confirms milestone 11 has eight open
issues and zero closed issues, all #50–#57 assigned correctly. Rust tests were
not run locally for these documentation-only changes; final-head CI runs the
required full locked two-thread workspace baseline. No compiler/runtime code changes
or new runtime support are claimed; the proposal remains Proposed.

Next unblocked proposal task: ADR1-01 (#50), deciding tactical and strategic
adoption separately. Production-session work can continue independently. Do not
merge the proposal PR without a later explicit user instruction.
## Immutable source module graphs — 2026-09-30

The earlier PR stack #42/#44/#45/#46/#48 was merged by the user. Current main
8686437 has the same committed tree as reviewed #48 fadb530; the new unpushed
portable-modules branch was aligned with that main without discarding edits.
Root independently completed the repaired #48 fresh portable oracle: 42 matching
source observations and 13 actual pipeline execution tests, exit 0;
/private/tmp/suss-definitions-reviewed-oracle.log. Reviewed-head CI 36661005170
passed on fadb5300aa681521477632ddb61149a00bb20364. M0/M1 remote milestones and
issues #1–#7 are closed; M2–M9 remain incomplete.

New portable::modules::prepare_modules discovers an immutable dependency graph
through the same namespace header grammar as fragment preparation. It checks all
source roots/extensions, declared namespaces and conditional branches, preserves
require order, deduplicates canonical phase identities and produces deterministic
located missing/mismatched/ambiguous/cyclic errors. A prototype guard rejects graph
nesting beyond 64. Compilation uses exact retained source text and private staged
Environments before returning dependency-first validated artifacts and cell IDs.
The caller's catalog/bindings cannot change on discovery or compilation failure.

Already-provided modules are explicit host authority, not inferred from a namespace
catalog entry. Runtime/macro identities are separate; canonical cljs.core uses the
bounded bootstrap core only if explicitly provided. The integration test host
validates every artifact before allocating cells, links every module before eval,
reuses shared cells in one Store and marks only successful initializers provided.
Diamond dependency writes execute once in require order. A failed initializer is
a verified ThrownException with the exact shared language tag and independently
inspected NotCallable descriptor after GC; prior writes/effects survive and retry
never replays a successful dependency. This is test host evidence, not shipped
session recovery or a complete source reload/cache/privacy policy.

Commands and terminal results (CARGO_BUILD_JOBS=2; no RUSTFLAGS override):

- `cargo test -p suss-compile --test portable_modules --test portable_definitions --test portable_closures --test portable_pipeline --test portable_resolution --test runtime_abi --locked -- --test-threads=2`: 64 passes with the initial ten module tests, zero ignored, exit 0; /private/tmp/suss-modules-focused.log. Initial harness compilation failed because ModulePlan lacked Debug and a test borrowed Store twice; both were repaired, not skipped.
- `cargo test -p suss-compile --test portable_modules --locked -- --test-threads=2`: 11 passes including initializer failure/retry, zero ignored, exit 0; /private/tmp/suss-modules-failure-focused.log. Combined focused scope is now 65 tests.
- `cargo test --workspace --locked -- --test-threads=2`: full baseline passed, exit 0; /private/tmp/suss-modules-full.log. Existing legacy/manual/doc ignores remain unchanged.
- `sh scripts/test-portable-pipeline-oracle.sh`: fresh pinned Node output matches all 42 portable source observations, followed by 13 actual pipeline tests, exit 0; /private/tmp/suss-modules-portable-oracle.log.
- `sh scripts/test-oracle.sh`: unchanged 9 differential passes / 7 exact failures / 0 skips, four executing oracle tests pass with one explicit manual recorder ignored, exit 0; /private/tmp/suss-modules-oracle.log.
- `python3 -m unittest discover -s scripts -p 'test_*.py'`: 47 passes, exit 0; /private/tmp/suss-modules-python.log.
- `python3 scripts/cljs_inventory.py --check` and `python3 scripts/cljs_reviews.py`: 1,065 declarations / zero reviewed / 1,065 unassessed, exit 0. `python3 scripts/wasi_lock.py`: 15 official WIT files / six packages verified, exit 0.

Implementation/tests are original; pinned EPL-1.0 cljs/analyzer.cljc and core.cljc
inform namespace/definition semantics, with no upstream implementation copied.
No new dependency, ABI layout change or shipped JVM/Node path is introduced.
See docs/runtime/portable-modules.md for API, host obligations and limits.

M2-01 and M3-02 gain prerequisite evidence only; no broad issue acceptance is
claimed. Publish with Refs #8/#13, dispatch an independent reviewer to push
significant fixes and require successful final-reviewed-head CI. Review/CI are
pending publication. Do not merge. Source require-macros and reload metadata remain
located unsupported diagnostics. Source caching/versioned macro dependency keys,
privacy/runtime attributes, isolated compiled macros, persistent production
CLI/AOT/REPL clients and full core/IR/target/lifecycle coverage remain unfinished.

Next unblocked task: connect immutable module plans and staged fragments to a
production persistent session with shared runtime/cells, distinct language-error
recovery, no source replay and explicit resident-code/reset policy. Continue the
full roadmap; do not retire legacy paths before replacement acceptance.

## Dispatched PR #49 review — 2026-09-30

Independent review at published 31f2aeff18c7ff76a1c6916235aa32ac8db62821 audited
shared namespace header parsing, source snapshots and staged compilation, DFS
require order/deduplication/cycles, canonical source ambiguity and declarations,
explicit provided authority and phase/core identities, diagnostics, artifact gates,
shared-cell initialization and typed failure/retry evidence. No significant
production defect was found. The existing diamond listed siblings alphabetically,
so an accidental dependency sort could pass its order assertions. The review
strengthens that regression to require right before left and checks both emitted
plan order and actual writes [1,3,2,4]. No compiler/runtime implementation changes.

A temporary alphabetical-sort mutation fails the strengthened test with the exact
wrong module order, exit 101; /private/tmp/suss-pr49-review-sorting-negative.log.
The mutation was restored completely and is not part of the PR. Root's fresh
pinned ClojureScript/Node reverse-order diamond independently observes
shared/right/left/app/result 1; /private/tmp/suss-modules-order-observations.txt.
An initial upstream fixture path setup failed before being corrected; that failure
is not called a successful execution. The probe is development-only and does not
claim full module/core compatibility.

Commands and terminal results (CARGO_BUILD_JOBS=2, shared CARGO_TARGET_DIR,
no RUSTFLAGS override):

- `cargo test -p suss-compile --test portable_modules --test portable_definitions --test portable_closures --test portable_pipeline --test portable_resolution --test runtime_abi --locked -- --test-threads=2`: published and repaired 65 passes, zero ignored, exit 0; /private/tmp/suss-pr49-review-focused.log and /private/tmp/suss-pr49-review-final-focused.log.
- `cargo test --workspace --locked -- --test-threads=2`: published and repaired baselines pass, exit 0; /private/tmp/suss-pr49-review-full.log and /private/tmp/suss-pr49-review-final-full.log. The repaired full run covers the final test/compiler tree; only documentation was edited while it ran.
- `python3 -m unittest discover -s scripts -p 'test_*.py'`: 47 pass, exit 0; /private/tmp/suss-pr49-review-python.log.
- Offline `python3 scripts/publish_roadmap.py`: pass, exit 0; /private/tmp/suss-pr49-review-roadmap-preview.json. Stable issue IDs retained.
- Touched-file rustfmt checks and `git diff --check`: pass.

No remaining significant review finding. Existing manual/legacy/doc ignores,
42-case portable corpus, legacy 9 passes/7 exact failures/0 skips and all 1,065
unassessed inventory entries are unchanged. No implementation was copied from
upstream; the test/evidence repair adds no dependency, ABI change or shipped
JVM/Node path. Production sessions, source reload/cache/privacy, compiled macros,
full core/IR/targets and M2/M3 acceptance remain incomplete. Next connect module
plans and fragments to the production persistent session with explicit error
recovery and resident-code/reset policy. Push the review repair and require CI on
the exact final reviewed head before readiness. Do not merge or close incomplete
roadmap packages.

## Native persistent session host — 2026-09-30

PR #49 remains open at independently reviewed 8e12024bd574326db0ff453274887dd3ec9eabdb;
final-head CI 36665168895 passed. Root reran the fresh pinned 42-case portable
oracle and actual pipeline artifacts on that reviewed head, exit 0;
/private/tmp/suss-modules-reviewed-portable-oracle.log. Linux CI logs explicitly
execute the reverse-order diamond and failed initializer/retry regressions;
/private/tmp/suss-modules-final-ci.log. PR/issue #8/#13 progress was updated to
reviewed-head readiness without claiming complete acceptance. No PR was merged.

The new suss_cli native library Session owns one Store/shared production runtime,
compiler Environment, live binding cells, successful module identities and resident
instances. Inputs compile through the portable pipeline; no source history is
stored/replayed. prepare_input stages optional leading-ns dependency graphs and the
input as one compiler transaction, sharing header/discovery/compilation code with
file plans. The host validates every artifact before cells, stages actual imports,
links all fragments before publication/eval and initializes in require order.

Owned SessionValue handles retain values/captures through later inputs/rebinding/GC.
Source errors change no session state. Failed initializers preserve old bindings
and completed effects; successful dependencies remain loaded, failed modules retry
without dependency replay. Exact language tags are taken/normalized separately
from traps/host errors; payloads remain rooted and the next input executes. Native
inspection callback errors also clear pending exception state and cannot classify
foreign tags as language success. Fuel budgets cover the whole operation, including
its dependencies. Reset replaces the Store/state and rejects old/foreign handles
before GC access or guest execution. load_namespace preserves caller scope;
input ns may set active scope and subsequent inputs preserve aliases/refers.

Counters report resident instances, sum of their input artifact sizes, cells,
provided modules excluding bootstrap core, current external handles and allocated
GC heap capacity. They do not measure actual JIT bytes, live objects or leaks.
Linked failed/uninitialized instances conservatively remain resident until reset.
No async scheduler, I/O cancellation or release acceptance is claimed.

Commands and terminal results (CARGO_BUILD_JOBS=2; no RUSTFLAGS override):

- Initial `cargo test -p suss-cli --test persistent_session --locked -- --test-threads=2` failed because the native library/API did not exist; /private/tmp/suss-session-red.log (also a consequent unknown-type inference diagnostic). Initial implemented host had 2 passes/2 failures on fixtures using dynamic global/parameter arithmetic; the existing compiler correctly reported located unsupported Number lowering. Fixtures now isolate persistence with literal writes/captures; no dynamic arithmetic repair or skip is claimed.
- `cargo test -p suss-cli --lib --test persistent_session --locked -- --test-threads=2`: 13 pass (11 integration/2 host regressions), zero ignored, exit 0; /private/tmp/suss-session-focused.log.
- `cargo test -p suss-compile --test portable_modules --test portable_definitions --test portable_closures --test portable_pipeline --test portable_resolution --test runtime_abi --locked -- --test-threads=2`: 65 pass, zero ignored, exit 0; /private/tmp/suss-session-compiler-focused.log. Combined focused scope: 78 tests.
- `cargo test --workspace --locked -- --test-threads=2`: final sequential run passed, exit 0; /private/tmp/suss-session-full-sequential.log. First full run passed runtime suites but failed CLI rustdoc E0463 while a concurrent oracle Cargo invocation rebuilt shared reader/compiler artifacts; /private/tmp/suss-session-full.log. After that process was terminal, the identical full command passed including the CLI doc check. No doctest was disabled or failure hidden. Avoid concurrent Cargo feature graphs sharing un-hashed reader artifacts.
- `sh scripts/test-portable-pipeline-oracle.sh`: fresh pinned output matches all 42 cases and 13 actual pipeline tests pass, exit 0; /private/tmp/suss-session-portable-oracle.log. Legacy differential baseline remains 9 passes/7 exact failures/0 skips in the full baseline; manual/legacy/doc ignores remain unchanged.
- `python3 -m unittest discover -s scripts -p 'test_*.py'`: 47 pass, exit 0; /private/tmp/suss-session-python.log. Inventory/review checks: 1,065 / zero reviewed / 1,065 unassessed, exit 0. WIT check: 15 files / six official packages verified, exit 0.
- Offline `python3 scripts/publish_roadmap.py`, touched Rust rustfmt checks and `git diff --check`: passed; stable issue IDs retained.

Tests independently inspect actual Number layouts/bits, UTF-16 units, sentinel
values and exception descriptors after GC. Reverse-order diamond effects execute
through shared source closures and cells. Corrupt later artifacts and missing
actual imports reject before binding publication/eval. Foreign/reset values are
rejected before runtime use; OutOfFuel remains an exact engine trap. No new test
is ignored. Implementation/fixtures are original; pinned EPL-1.0 cljs namespace/def
semantics informed the contract, with no upstream implementation copied.

Cargo.lock changes only the suss-cli dependency list to include already-locked
workspace tempfile for development tests; no package version changes. No shipped
Java/Node path or runtime ABI layout change. Proposal-only ADR work in a separate
user turn did not change the accepted contract and is outside this implementation.
See docs/runtime/portable-session.md for API, lifecycle and limits.

Publish stacked on #49 with Refs #10/#12/#13/#15; this is runtime/host prerequisite
evidence, not completed M3 delivery. Dispatch an independent reviewer to reproduce
and push significant fixes; require CI on the exact final reviewed head. Review/CI
are pending publication. Do not merge or close these incomplete packages.

Remaining: actual production command/REPL frontend migration and printing, atoms/
nominal types/collections, extended signatures and dynamic arithmetic/core coercions,
full exception/effect/recur IR, source declaration/unbound-var certification,
reload/cache/privacy and compiled macros, async I/O/cancellation/live heap counters,
WIT/browser/CSP targets. Next connect the actual REPL frontend to this host as
portable lowering/printing coverage reaches replacement acceptance; dynamic
numeric/global/parameter lowering is an immediate compiler prerequisite. Retire
source replay/obsolete paths only after replacement acceptance. M2–M9 remain open.

## PR #59 independent review — 2026-09-30

Reviewed the published portable-session head 81cfdad in an isolated worktree.
Found a native callback recovery gap: a callback that translates a thrown exception
into an ordinary host error left the Store carrying its pending exception. The
new focused regression reproduced it before repair, exit 101;
/private/tmp/suss-pr59-review-red.log. The host now clears pending exception state
when propagating translated errors and rejects a callback that returns success
while leaving a pending exception. Properly propagated exceptions still use exact
tag identity; foreign tags remain host errors and traps remain distinct.

The regression covers translated error and swallowed-throw success paths, exact
host messages, pending state removal, unchanged external root count and an
independently decoded next-input Number 42. An intermediate repair failed to compile
because RootScope does not expose has_pending_exception directly; corrected through
StoreContextMut, with no skipped test or weakened assertion.

Commands and terminal results (CARGO_BUILD_JOBS=2, shared CARGO_TARGET_DIR, no
RUSTFLAGS override; Cargo invocations sequential):

- `cargo test -p suss-cli --lib --test persistent_session --locked -- --test-threads=2`: 14 pass, zero ignored, exit 0; /private/tmp/suss-pr59-review-session-focused.log.
- `cargo test -p suss-compile --test portable_modules --test portable_definitions --test portable_closures --test portable_pipeline --test portable_resolution --test runtime_abi --locked -- --test-threads=2`: 65 pass, zero ignored, exit 0; /private/tmp/suss-pr59-review-compiler-focused.log. Combined focused scope: 79 tests.
- `cargo test --workspace --locked -- --test-threads=2`: pass including CLI rustdoc, exit 0; /private/tmp/suss-pr59-review-full.log. No overlapping Cargo invocation.
- `python3 -m unittest discover -s scripts -p 'test_*.py'`: 47 pass, exit 0; /private/tmp/suss-pr59-review-python.log.
- Offline `python3 scripts/publish_roadmap.py`: pass, exit 0; /private/tmp/suss-pr59-review-roadmap-preview.json. Stable issue IDs retained.
- Touched Rust rustfmt and `git diff --check`: pass.

No remaining significant review finding. No upstream implementation copied,
dependency changes, runtime ABI changes or source-semantic changes in the review repair. Existing unassessed inventory,
portable oracle and legacy differential scope remain unchanged. Production
command/REPL migration, dynamic arithmetic/core lowering, full M2/M3 acceptance,
async cancellation and live heap accounting remain incomplete. Next implement the
compiler prerequisites for frontend migration, then replace source replay after
replacement acceptance. Require CI on the exact final reviewed head; do not merge
or close incomplete issues.

## Unary sum/product identity — 2026-09-30

PR #59's dispatched reviewer pushed 90bcd8c91fa5d65d84c0b57d1f82b7c8925d3638,
repairing pending exceptions after native callbacks translate or swallow a throw.
Fourteen session and 65 compiler checks (79 focused), the sequential full baseline,
Python47 and roadmap preview pass. Root synced the exact review fix and reran
fresh pinned 42-case observations plus 13 actual pipeline tests, exit 0;
/private/tmp/suss-session-reviewed-portable-oracle.log. Final-head Linux CI
36669735862 is still running at this point; readiness requires its terminal result.

The pinned core source specifies one-argument + and * as identity, even for
non-number values. The portable analyzer incorrectly demanded a proven Number
and assigned a Number result. It now retains the operand type for precisely these
arities; existing IR returns the evaluated operand directly. All actual numeric
operations retain their existing Number guard. No runtime ABI change, implicit
coercion or new checked-Number semantic substitute is introduced.

The original regression fails before the repair at (+ nil), with located unsupported
Number lowering (exit 101); /private/tmp/suss-unary-red.log. Its first draft failed
to compile due to incorrect reader/analyzer API names, corrected before recording
the semantic red. The fresh reference corpus was expanded without dropping cases:
54 pinned observations match, then Suss fails the new cases before the repair
(12 pipeline tests pass/2 fail); /private/tmp/suss-unary-oracle-red.log. The pinned
compiler's non-number arithmetic warnings remain visible, with successful exact
values. After the repair, an old hard-coded corpus-count assertion still expected
42; updated to 54 without weakening decoding/comparison or changing expectations.

New executing cases cover nil, booleans, UTF-16/lone surrogates, dynamic parameters/
globals, nested identity, mixed-type branches and returned closures. HIR type and
known closure-arity checks distinguish identity from a fabricated Number result;
a subsequent binary String arithmetic call remains a located unsupported error.
The production session invokes old captured closures after rebinding/GC, verifies
completed effects, current global function lookup and independently decodes sentinel/
string values after GC. Tests are original; no upstream forms were copied. Semantic
provenance: pinned c4295f303100bbf5afac449242d30bca1126f1a1 EPL-1.0 core.cljs
2724–2744 and arithmetic macros in core.cljc.

Commands and terminal results (CARGO_BUILD_JOBS=2; no RUSTFLAGS override; Cargo
feature graphs run sequentially):

- `cargo test -p suss-compile --test portable_modules --test portable_definitions --test portable_closures --test portable_pipeline --test portable_resolution --test runtime_abi --locked -- --test-threads=2`: 66 pass, zero ignored, exit 0; /private/tmp/suss-unary-compiler-focused.log.
- `cargo test -p suss-cli --lib --test persistent_session --locked -- --test-threads=2`: 15 pass, zero ignored, exit 0; /private/tmp/suss-unary-session-focused.log. Combined focused scope: 81 tests.
- `sh scripts/test-portable-pipeline-oracle.sh`: fresh 54-case pinned source observations match and 14 pipeline tests pass, exit 0; /private/tmp/suss-unary-portable-oracle.log.
- `python3 -m unittest discover -s scripts -p 'test_*.py'`: 47 pass, exit 0; /private/tmp/suss-unary-python.log. Inventory/review verification: 1,065 / zero reviewed / 1,065 unassessed.
- `python3 scripts/wasi_lock.py`: 15 files/six official WIT packages verified, exit 0. An initial command used a nonexistent check_wit_packages.py name and failed; the repository's actual verifier was then run successfully.
- Offline roadmap preview, touched Rust formatting and `git diff --check` pass; stable issue IDs and statuses retained.

`cargo test --workspace --locked -- --test-threads=2` completed successfully,
exit 0 including CLI rustdoc; /private/tmp/suss-unary-full.log. Legacy differential
baseline remains 9 passes/7 exact failures/0 skips; existing ignores are unchanged.
Publish a PR stacked on #59 with Refs #9/#10.
Dispatch an independent review with significant fixes pushed, then require exact
reviewed-head CI. Do not merge or close incomplete issues. The broad M2/M3 gates,
actual command frontend, binary arithmetic/coercions, extended signatures,
collections/recur/effects/macros and all later milestone acceptance remain open.
Next implement dynamic arithmetic with the pinned coercion contract; unary identity
must not be replaced by a Number type assertion. Retire legacy source replay only
after replacement acceptance.
