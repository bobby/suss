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

## PR #60 independent review — 2026-09-30

Reviewed published e73f544 in an isolated worktree against the accepted portable
contract and pinned EPL-1.0 core.cljs unary definitions. Audited analyzer arity/type
propagation, identity IR lowering, closure arity/captures, dynamic globals and
parameters, mixed branch types, independent scalar decoders, owned session values
through GC/rebinding, strict 54-case transport and remaining unsupported numeric
operations. No significant production defect was found.

The new session effect fixture assigns an identical literal, so it cannot alone
detect accidental repeated operand evaluation. Strengthened the existing ordered
runtime import trace by wrapping effectful operands in unary + and *. It observes
exactly multiply/divide/multiply/add, retains the unselected-branch assertion and
independently checks Number 14. A temporary duplicate-evaluation IR mutation fails
with repeated multiply/divide calls, exit 101;
/private/tmp/suss-pr60-review-duplicate-negative.log. The mutation was restored
entirely before final checks; compiler implementation is unchanged in this repair.

Commands and terminal results (CARGO_BUILD_JOBS=2, shared CARGO_TARGET_DIR,
no RUSTFLAGS override; Cargo feature graphs ran sequentially):

- `cargo test -p suss-compile --test portable_modules --test portable_definitions --test portable_closures --test portable_pipeline --test portable_resolution --test runtime_abi --locked -- --test-threads=2`: 66 pass, zero ignored, exit 0; /private/tmp/suss-pr60-review-compiler-focused.log.
- `cargo test -p suss-cli --lib --test persistent_session --locked -- --test-threads=2`: 15 pass, zero ignored, exit 0; /private/tmp/suss-pr60-review-session-focused.log. Combined focused scope: 81 tests.
- `cargo test --workspace --locked -- --test-threads=2`: pass including CLI rustdoc, exit 0; /private/tmp/suss-pr60-review-full.log. Code was frozen for the full run.
- `python3 -m unittest discover -s scripts -p 'test_*.py'`: 47 pass, exit 0; /private/tmp/suss-pr60-review-python.log.
- Offline `python3 scripts/publish_roadmap.py`: pass, exit 0; /private/tmp/suss-pr60-review-roadmap-preview.json. Stable issue IDs retained.
- Touched Rust rustfmt and `git diff --check`: pass.

PR #59 final-head CI 36669735862 is now confirmed successful on independently
reviewed 90bcd8c91fa5d65d84c0b57d1f82b7c8925d3638. Root verified its executing
callback recovery/session regressions and updated its PR and issue progress;
the PR remains open. PR #60 still requires CI on the exact final reviewed head.

No remaining significant review finding. This original test/evidence repair adds
no dependencies, ABI change, upstream implementation or shipped JVM/Node path.
All existing ignores, inventory statuses and 54 portable cases remain unchanged.
M2/M3 acceptance, command frontend migration and later milestones remain open.
Next implement dynamic arithmetic with pinned primitive coercion/concatenation
semantics; a blanket Number guard would not fulfill the contract. Do not merge
or close incomplete issues.

## Primitive dynamic arithmetic — 2026-09-30

PR #59 final reviewed head 90bcd8c91fa5d65d84c0b57d1f82b7c8925d3638 passed CI
36669735862. PR #60 final reviewed head 12f66581315effe943b4f05c8bb0e9f1f2336cd5
passed CI 36671317970; predecessor run 36670850506 was cancelled, not successful.
Both remain open, as does #49. This increment branches from reviewed #60 and will
use Refs #9 and Refs #10. Neither issue's full acceptance is completed here.

Portable HIR/IR now retains primitive arithmetic result types and chooses checked
value intrinsics for dynamic globals/parameters and non-number primitives. Proven
Number operands retain the fast path. Unary +/* remain identity. Nil/booleans and
UTF-16 strings follow pinned primitive Number conversion; addition concatenates
when either primitive is a string. Operand expressions are still evaluated once
in source order before calls. Known closure coercions have located unsupported
diagnostics; dynamic object conversion throws explicit descriptor 5. First-class
arithmetic bindings and complete object/core conversion remain future work.

A private allocator-free Rust helper implements original ECMAScript grammar,
whitespace and correctly rounded radix parsing, Rust core decimal conversion and
pinned ryu-js 1.0.3 shortest formatting. It has no host imports, JVM/Node, allocator,
table or start. Its ordinary types/global references relocate after the unchanged
recursive GC prelude. Separate private linear memory holds Rust stack/static data
and reusable checked scratch; it is not canonical component memory. Allocation
failure throws descriptor 6. GC strings are copied, never modified. Stack pointer
reset before every non-reentrant helper call recovers after an actual fuel trap.
SessionStats adds numeric_memory_capacity; reset replaces it with the Store.

Source/dependency/toolchain/license/Wasm hashes and exact compiler commit are pinned
in runtime/numeric/artifact/manifest.json. Byte-preserved ryu-js license texts and
the toolchain's complete library copyright report are retained with NOTICE. The
report intentionally includes notices broader than linked code. Distributions of
the generated runtime must retain these files. No ClojureScript implementation was
copied; pinned EPL-1.0 source informs semantics/development oracles only.

Commands/results (CARGO_BUILD_JOBS=2, no RUSTFLAGS override; native Cargo runs
sequentially; private helper builds use an isolated temporary target):

- Pre-implementation pipeline regression failed at (+ nil 1), exit 101; /private/tmp/suss-primitive-arithmetic-red.log. An initial encoder compile failed on a u32/i64 constant mismatch and was corrected. Existing negative arithmetic fixtures were then updated to test unsupported object coercion rather than reject newly supported primitives; no skip was introduced.
- Compiler focused modules/definitions/closures/pipeline/resolution/runtime_abi suite: 68 passed, zero ignored, exit 0; /private/tmp/suss-primitive-compiler-focused.log.
- `cargo test -p suss-cli --lib --test persistent_session --locked -- --test-threads=2`: 18 passed (14 integration/4 private), zero ignored, exit 0; /private/tmp/suss-primitive-session-final.log. Combined focused scope: 86.
- `sh scripts/test-portable-pipeline-oracle.sh`: all 158 expanded source cases match fresh pinned observations; 14 independently decoded pipeline tests passed, exit 0; /private/tmp/suss-primitive-portable-oracle.log. All prior 54 cases are retained. Parsing grammar/whitespace/radix rounding/subnormal/overflow/formatting boundaries and UTF-16 concatenation are covered.
- `sh scripts/test-numeric-oracle.sh`: 1,024 unique binary64 inputs match fresh pinned ClojureScript formatting/round-trip observations and actual GC runtime execution, exit 0; /private/tmp/suss-primitive-numeric-oracle.log. Strict tagged transport remains exact; sampling is not exhaustive numeric compatibility.
- A temporary removal of both stack-reset instructions fails the interrupted Rust-frame recovery regression, exit 101 (1048560 versus initial 1048576); /private/tmp/suss-primitive-stack-reset-negative.log. Mutation fully restored. The initial test incorrectly assumed inspect starts an operation fuel budget; corrected to set the actual Store fuel, not weaken the runtime check.
- Allocation-limited runtime regression passes with exact shared language tag/descriptor/message, unchanged capacity and a successful next independently decoded value; /private/tmp/suss-primitive-allocation-focused.log.
- `python3 scripts/numeric_runtime.py --record`, then `--rebuild-check`: successful byte-identical pinned builds, including final retained notice hashes; /private/tmp/suss-primitive-notices-record.log and /private/tmp/suss-primitive-notices-rebuild.log.
- Python regression suite: 54 passed, exit 0; /private/tmp/suss-primitive-python-final.log. Inventory/review: 1,065 declarations/0 reviewed/1,065 unassessed. WIT: 15 files/six packages. Numeric integrity, offline roadmap preview, touched Rust formatting and git diff checks pass. Stable issue IDs/statuses retained.
- Full `cargo test --workspace --locked -- --test-threads=2`: passed, exit 0 including CLI rustdoc; /private/tmp/suss-primitive-full.log. No overlapping native Cargo feature graph. Existing legacy/manual/doc ignores remain unchanged.

Remaining: extended signatures, first-class core arithmetic and object conversion,
collections/dispatch/recur/effect IR, full ExceptionInfo, production command/REPL
migration and printing, reload/cache/privacy, compiled macros, async I/O/cancellation,
canonical memory and target packaging. Legacy differential evidence stays separate:
9 passes/7 exact failures/0 skips; inventory remains unassessed. M2–M9 remain open.
Next remove the first-class arithmetic binding restriction while preserving unary
identity, dynamic coercion, central invocation/arity and once-only evaluation; then
continue the shared frontend/core prerequisites. Do not retire legacy paths until
replacement acceptance. Publish stacked on #60, dispatch independent PR review,
push significant fixes and require CI on the exact final reviewed head. Do not merge.

## PR #61 independent review — 2026-09-30

Reviewed published bb446608e459f5d8696872a36391f04e7143e8b1 in an isolated
worktree against the accepted portable contract. Audited primitive arithmetic
type propagation, source evaluation order, helper type/global relocation,
no-import/no-allocator shape, checked scratch growth, typed allocation failures,
interrupted Rust stack recovery, immutable GC strings and session reset. No
significant production semantic defect was found in this bounded primitive scope.

Found a reproducibility defect in the new numeric oracle runner: its transport
namespace requires generated suss-oracle.cases, but the runner generated only
numeric-cases. In the fresh worktree the exact runner failed with missing
suss-oracle.cases, exit 1; /private/tmp/suss-pr61-review-numeric-clean-red.log.
The runner now generates the shared base corpus before compiling the oracle.
It then passed fresh pinned 1,024-row observations and actual GC runtime execution,
exit 0; /private/tmp/suss-pr61-review-numeric-clean-fixed.log. The isolated worktree
used the existing clean pinned submodule as a read-only source reference.

Strengthened the existing verifier regression to reject String and dynamic Value
arithmetic results forged as Number. Both verification and compile_ir reject
these graphs, preventing unchecked Number assumptions in later lowering. Removing
the arithmetic result check temporarily makes this regression fail, exit 101;
/private/tmp/suss-pr61-review-arithmetic-type-negative.log. The mutation was fully
restored before final checks. Also removed the executable bit from the numeric
Wasm build input; its bytes and manifest hash remain unchanged.

Commands and terminal results (CARGO_BUILD_JOBS=2, no RUSTFLAGS override;
native Cargo feature graphs run sequentially in the isolated worktree):

- Compiler modules/definitions/closures/pipeline/resolution/runtime_abi focused suite: 68 pass, zero ignored, exit 0; /private/tmp/suss-pr61-review-compiler-final.log.
- `cargo test -p suss-cli --lib --test persistent_session --locked -- --test-threads=2`: 18 pass (four private/fourteen integration), zero ignored, exit 0; /private/tmp/suss-pr61-review-session-focused.log. Combined focused scope: 86.
- `cargo test --workspace --locked -- --test-threads=2`: pass including CLI rustdoc, exit 0; /private/tmp/suss-pr61-review-full.log. Code was frozen during the full run; no overlapping Cargo invocation or new ignore.
- Fixed `sh scripts/test-numeric-oracle.sh`: fresh pinned 1,024-sample comparison and actual runtime test pass, exit 0; /private/tmp/suss-pr61-review-numeric-clean-fixed.log.
- `python3 scripts/numeric_runtime.py --rebuild-check`: byte-identical pinned helper rebuild, exit 0; /private/tmp/suss-pr61-review-helper-rebuild.log. Integrity check still passes after the file-mode repair.
- Python regression suite: 54 pass, exit 0; /private/tmp/suss-pr61-review-python-final.log.
- Inventory/review checks: 1,065 declarations/zero reviewed/1,065 unassessed. WIT: 15 files/six official packages. Offline roadmap preview, touched Rust formatting, shell syntax and git diff checks pass; stable issue IDs/statuses retained.

Predecessor CI 36677683283 succeeded on bb446608e459f5d8696872a36391f04e7143e8b1;
it does not certify this review repair. Require a new successful CI run on the
exact final reviewed head before readiness. No remaining significant review finding.
The repair changes no production semantics, dependencies, ABI layouts or upstream
implementation. All 158 source cases and 1,024 numeric samples remain intact;
legacy differential evidence remains 9 passes/7 exact failures/zero skips.
Object conversion, first-class arithmetic bindings, complete public numeric core,
production frontend migration and M2–M9 acceptance remain unfinished. Next remove
the first-class arithmetic restriction with central invocation/arity, canonical
live cells and pinned variadic semantics; preserve once-only evaluation and unary
identity. Keep Refs #9/#10 rather than closing incomplete issues. Do not merge.

## Arithmetic function values: in-progress follow-up — 2026-09-30

Isolated worktree /private/tmp/suss-arithmetic-values on
resurrection/portable-arithmetic-values, based on independently reviewed PR #61
cf6eb51c577834ece2def89fa7ce7f094b2b0930. Root fresh source/numeric oracles passed
on that exact predecessor (158 source observations/14 pipeline tests and 1,024
numeric samples/one actual runtime test). Final predecessor CI 36679120849 passed on that exact reviewed head.
Root verified its actual runtime/verifier/session execution logs and updated the
PR and issues #9/#10 to reviewed-head readiness; PR #61 remains unmerged. No merge and no successor PR yet.

New native source regression fails with the expected located unsupported core
function-value diagnostic at (let [f +] (f)), exit 101;
/private/tmp/suss-arithmetic-values-red.log. It covers zero/unary/variadic calls,
computed/returned functions, closure identity behavior, higher-order primitive
coercion and wrong-arity recovery. Source materialization remains unfinished.

Runtime closure factories now create add/subtract/multiply/divide functions with
shared Invoke type, minimum arities 0/1 and variadic maximum -1. They preserve
unary +/* identity, negate/reciprocate unary -// and fold evaluated arguments left
to right through checked primitive intrinsics. No upstream core source copied;
semantic provenance remains pinned core.cljs 2724–2753, EPL notices upstream.
Central invoke guards source arity; trusted direct invoker calls also diagnose
empty -//. No Java/Node or new shipped dependency.

An initial implementation used an appended function type with an identical
signature, but it does not share the recursive group's Invoke identity; seven
runtime tests failed actual Wasm validation. Fixed by assigning the invoke bodies
the exact prelude Invoke type, with declarative ref.func elements. No failure
hidden: /private/tmp/suss-arithmetic-values-runtime-focused.log records red, and
/private/tmp/suss-arithmetic-values-runtime-focused-fixed.log records all nine
existing runtime tests passing, exit 0. New independent GC/arity test passes,
exit 0; /private/tmp/suss-arithmetic-values-invocation-focused.log. It checks empty,
unary and ordered variadic results after GC, signed-zero bits, min/max closure
fields, actual exception tag and wrong-arity descriptor.

All native Cargo invocations are sequential with CARGO_BUILD_JOBS=2 and shared
CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target; no RUSTFLAGS override.
No full successor baseline yet; source regression remains intentionally red until
implementation. Do not publish or call this follow-up ready from runtime checks.

Fresh pinned private probes establish 18 source arithmetic-function scalar
observations (/private/tmp/suss-first-class-arithmetic-probe-inputs.json and
-observations.json). Initial generated fixture path was wrong and namespace lookup
failed; corrected ignored path then compiled/executed successfully. These are
reference-only results, not Suss passes. A separate temporary core redefinition
probe confirms direct arithmetic macro calls remain inlined while function-value
lookup sees replacement. Captured original + exposed an upstream generated-JS
arity-property TypeError under a fixed-arity replacement; preserved explicitly
in /private/tmp/suss-core-arithmetic-redefinition-observations.json, not converted
to success. Do not adopt JS wrapper dispatch defects over the accepted captured
function guarantee. Canonical identity and unary closure identity are true.

Next provision stable canonical arithmetic cells, preserve phase isolation,
suss.core/cljs.core aliases, redefinition and original captured functions. Resolve
arithmetic values to those live cells; a per-use closure constant is insufficient.
Add the pinned scalar cases to the common corpus and execute actual fragments.
Account honestly for resident bootstrap cells in SessionStats. Complete focused
and full checks, docs/inventory evidence, linked PR, independent review/fixes and
final-head CI. All larger M2–M9 acceptance remains open.

Source lookup now materializes used arithmetic identities in the staged compiler
Environment and reads live cells. Native Session provisions four canonical
runtime-phase arithmetic cells once per Store; current aliases share those cells
and reset reprovisions them. Source function-value regression now passes, exit 0;
/private/tmp/suss-arithmetic-values-source-focused.log. The full compiler focused
scope passes 69 tests, exit 0; /private/tmp/suss-arithmetic-values-compiler-focused.log.
The initial full session focus had 18 passes/one failure because reset previously
expected zero cells; the four new bootstrap cells are resident and reported
truthfully. The reset assertion now compares against the explicitly checked four
startup cells, not a hidden counter subtraction. Session recheck is pending.

Still required before successor publication: canonical identity/phase isolation,
core/user redefinition and captured-old-function regressions, shared source corpus
expansion/fresh pinned execution, independent decoder host provisioning, metric/API
docs, full baseline, inventory alignment and independent PR review/final CI. No
full compatibility or successor readiness claim follows from this compiler focus.

Final session recheck passed 19 tests (15 integration/4 private), zero ignored,
exit 0; /private/tmp/suss-arithmetic-values-session-focused-fixed.log. Combined
current focused scope is 88 (69 compiler/19 session). The source materialization
regression is now green; the missing-source red above is pre-implementation evidence.
No native Cargo process remains live. Successor changes are uncommitted in the
isolated worktree, with required remaining gates listed above. Root worktree stays
on PR #61 reviewed cf6eb51 with only the two unrelated reference files untracked.

## Arithmetic function values: publication validation — 2026-09-30

The source/runtime implementation now executes arithmetic functions as values
through canonical phase-specific live cells and universal invocation. Native
Session seeds four runtime cells per Store; statistics count them, and reset
reprovisions them. Repeated alias reads and unary identity retain exact GC reference
identity. Old owned functions and source captures retain original behavior after
core replacement and GC; subsequent function-value lookup sees the replacement.
Callee and argument effects execute exactly once in order. A compile failure leaves
session state unchanged. Phase fixtures execute distinct runtime/macro cell contents.

The original 158 source cases are retained and 18 pinned reference function-value
cases added (176 total). The executing corpus host explicitly initializes canonical
arithmetic cells with the runtime factories. Dynamic object coercion remains an
explicit unsupported boundary. Extended source signatures and compiled macros/core
import remain incomplete; direct bootstrap arithmetic is not generalized macro
expansion. No obsolete frontend is retired or complete milestone claimed.

Four public runtime arithmetic declarations have manual in-progress adapted
reviews with exact source hashes, fixed/variadic arities, macro/reduction dependencies,
original adaptation path and executing references. 1,061 remain unassessed; no
review is marked implemented or excluded. An initial review-generation assertion
used macro:divide:1160; the actual stable inventory ID is macro:divide:1159, corrected
before writing the overlay. Generated inventory is unchanged. No core form copied,
new dependency/lock change or shipped Java/Node path. Existing helper notices remain.

Commands/results (CARGO_BUILD_JOBS=2, CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target,
no RUSTFLAGS override; native feature graphs sequential):

- Compiler modules/definitions/closures/pipeline/resolution/runtime_abi focus: 70 pass, zero ignored, exit 0; /private/tmp/suss-arithmetic-values-compiler-final.log.
- Native Session library/persistent_session focus: 21 pass (17 integration/4 private), zero ignored, exit 0; /private/tmp/suss-arithmetic-values-session-final.log. Initial new reference-identity fixture failed to compile because unwrap_anyref already returns a reference; removed the extra reference, then exact ref_eq observations passed. Total focused scope: 91.
- `sh scripts/test-portable-pipeline-oracle.sh`: fresh pinned 176-case observations match and all 14 independently decoded pipeline tests pass, exit 0; /private/tmp/suss-arithmetic-values-portable-oracle.log. Warnings for upstream non-number arithmetic are retained.
- Python suite: 54 pass, exit 0; /private/tmp/suss-arithmetic-values-python-final.log. Inventory: 1,065 source declarations verified. Review overlay: four reviewed as in-progress/1,061 unassessed. WIT: 15 files/six packages. Numeric artifact integrity, offline roadmap preview and diff checks pass. Stable issue IDs/statuses retained.
- Required full `cargo test --workspace --locked -- --test-threads=2`: passed, exit 0 including CLI rustdoc; /private/tmp/suss-arithmetic-values-full.log. No overlapping native Cargo feature graph. Existing ignored/manual checks remain unchanged.

Publish stacked on reviewed PR #61 with Refs #9/#10, dispatch an independent reviewer
who pushes significant fixes, then require exact final-head CI. No PR or issue closes
from these partial gates. Existing legacy differential 9 passes/7 exact failures/0
skips and manual/legacy/doc ignores remain separate and unchanged. Next complete
extended closure signatures and general control flow/core foundations needed by
source import and production frontend replacement. M2–M9 remain open.

## PR #62 independent review — 2026-09-30

Reviewed 23c33d4 in isolated /private/tmp/suss-review-pr62 against the accepted
design and reviewed predecessor cf6eb51. Audited phase-specific canonical arithmetic
cells, staged HIR materialization, module-plan cell aggregation, core redefinition,
original captures, GC identity, native bootstrap/reset counters, shared recursive
Invoke identity and declarative ref.func elements. Checked central minimum/variadic
arity, unary identity and primitive left-fold coercions. Existing universal-call
tests observe an exact ordered callee/read/invoke trace; arithmetic session tests
also execute callee/argument effects once. Source oracle provisioning and the four
manual in-progress inventory entries retain honest dependencies and provenance.
No significant production defect or remaining review finding was found in this
bounded increment. No semantic change, dependency, ignore or upstream source copy
was introduced by review.

Commands/results (CARGO_BUILD_JOBS=2, shared CARGO_TARGET_DIR, no RUSTFLAGS override;
native Cargo feature graphs run sequentially):

- Compiler modules/definitions/closures/pipeline/resolution/runtime_abi focused suite: 70 pass, zero ignored, exit 0; /private/tmp/suss-pr62-review-compiler.log.
- Native Session library/persistent_session focus: 21 pass (four private/seventeen integration), zero ignored, exit 0; /private/tmp/suss-pr62-review-session.log. Combined focused scope: 91.
- Required `cargo test --workspace --locked -- --test-threads=2`: passes including CLI rustdoc, exit 0; /private/tmp/suss-pr62-review-full.log. Code remained frozen during this run; existing legacy/manual/doc ignores unchanged.
- Fresh pinned source reference regenerated with oracle_cases.py, portable_oracle.py generate, cljs.main and Node, then portable_oracle.py compare: 176 observations match, exit 0; /private/tmp/suss-pr62-review-oracle-comparison.log. The compiler focus independently executes and decodes all 176 cases in its fourteen pipeline tests. The clean pinned root submodule was a read-only source reference. Upstream non-number arithmetic warnings remain visible.
- Python regression suite: 54 pass, exit 0; /private/tmp/suss-pr62-review-python.log.
- Inventory/review: 1,065 declarations/four in-progress reviews/1,061 unassessed. WIT: fifteen files/six packages. Numeric artifact integrity, offline roadmap preview and touched Rust formatting with workspace edition 2024 pass. An initial formatting invocation incorrectly selected edition 2021; corrected to the actual workspace edition, without changing source.

This review records evidence only. Require successful CI on the exact final
reviewed head, including this handoff update, before readiness. Keep Refs #9/#10:
object coercion, full core/macro import, extended signatures, production frontend
migration and M2–M9 acceptance remain unfinished. The next unblocked task is
extended closure signatures and control-flow/core foundations; no milestone or
issue closes from this partial increment. Do not merge.

## Source loop/recur: pre-implementation evidence — 2026-09-30

Successor branch resurrection/portable-recur in /private/tmp/suss-portable-recur
starts from PR #62's independently reviewed e22b45d94fc35746e856fabb51b5350ff26d47cc.
The reviewer reproduced 91 focused checks, Python54, full workspace baseline and
fresh 176 observations, found no significant defect, and pushed review evidence.
Predecessor CI 36681661474 was cancelled. Exact reviewed-head CI 36682291901 passed on e22b45d94fc35746e856fabb51b5350ff26d47cc.
Root verified executing arithmetic/phase/capture regressions and updated the PR
and issues #9/#10 to reviewed-head readiness. No PR merged.

Added a focused native Session regression for sequential loop initializers,
parallel recur swaps, changing parameter types, function recur and zero bindings.
It fails with the existing located Unresolved Runtime name loop diagnostic,
exit 101; /private/tmp/suss-portable-recur-red.log. Native Cargo is terminal.
No source implementation or successor PR exists yet; do not skip that red test.

Fresh pinned ClojureScript reference probes execute 14 scalar cases covering
these boundaries plus shadowing, empty body, nil, truthy zero, nested targets,
tail let and ordered replacement effects. Exact tagged observations are validated:
/private/tmp/suss-recur-probe-inputs.json, /private/tmp/suss-recur-probe-observations.json,
and /private/tmp/suss-recur-reference-probe.log. These are reference-only observations,
not compiler compatibility passes. The implementation plan is recorded in
/private/tmp/suss-recur-implementation-plan.md.

Next add lexical target identity and true tail-position analysis, isolating fn
boundaries; reject outside-target, non-tail and arity errors with spans. Lower
initializers/replacements once into explicit header parameters and backedges;
all replacement values must precede assignments. Represent diverging flow without
fabricated nil or unreachable joins. Existing IR edge emission already supports
parallel assignment, but source recurrence does not yet reach it. Add executed
positive/negative, nested capture, changing-type and fuel/recovery regressions;
then fresh common corpus, focused/full checks, docs/inventory evidence, linked PR,
independent review/pushed fixes and exact final-head CI. M2–M9 remain unfinished.

Four additional fresh pinned recurrence reference cases validate original closure
captures from a prior iteration, outer captures through function recur, nested
function target isolation and changing Number-to-String arithmetic (result UTF-16
"21"). Exact inputs/observations are /private/tmp/suss-recur-capture-probe-inputs.json
and -observations.json; log /private/tmp/suss-recur-capture-reference-probe.log.
All 18 reference rows are strict-tag validated, with no Suss pass claimed.
No native/reference/CI handle remains live. Successor changes are uncommitted in
this isolated worktree; PR #62's worktree remains clean at reviewed e22b45d.

## Source loop/recur: implementation validation — 2026-09-30

Simple-symbol loop/loop* and fixed-function recur now lower into explicit header
parameters/backedges. Initializers and replacements evaluate once in source order;
all edge values precede writes. Tail position and nearest target are checked in
source analysis and independently for public HIR, with function isolation and
located arity/outside/non-tail errors. Function bodies use synthetic loops with
fresh parameter identities; old iteration captures and immutable outer captures
remain stable. Mutable headers use Value facts. Divergent flow has no fabricated
nil or unreachable joins; fuel traps remain separate and subsequent input recovers.

The original unresolved-loop regression now passes. Additional source/HIR negative,
parallel/capture/effect/type/GC and zero-parameter fuel tests execute. An initial
new session test used nonexistent Session::call; corrected to Session::invoke.
An initial compiler test treated Diagnostic as a vector; corrected its fixture.
These compile failures are preserved in /private/tmp/suss-portable-recur-session.log
and -compiler.log, not skipped. cargo fmt --all exposed unrelated legacy format
changes; restored those bytes from HEAD and formatted only touched Rust files.

Sequential native commands use CARGO_BUILD_JOBS=2 and shared CARGO_TARGET_DIR, with
no RUSTFLAGS override. Results: compiler six-suite focus 72 pass/zero ignored,
exit 0, /private/tmp/suss-portable-recur-compiler-fixed.log; native integration
20 pass/zero ignored, exit 0, /private/tmp/suss-portable-recur-session-fixed.log.
Fresh pinned source oracle now has 194 exact observations, with 16 independently
decoded pipeline tests passing, exit 0, /private/tmp/suss-portable-recur-oracle.log.
Python54 pass, /private/tmp/suss-portable-recur-python.log. Inventory1065 with four
in-progress reviews/1061 unassessed, WIT15/six packages, numeric integrity, offline
roadmap preview and touched formatting/diff gates pass. No core source copied,
ABI/dependency/helper artifact change or shipped JVM/Node introduced. Provenance
and limits are in docs/runtime/recurrence.md; full upstream loop destructuring
and compiled macro behavior are still unassessed.

Required full workspace baseline completed successfully, exit 0 including CLI
rustdoc; /private/tmp/suss-portable-recur-full.log. Native session79919 is terminal.
No native Cargo feature graph overlapped this run.
Publish stacked on #62 with Refs #9/#10, independent review/pushed fixes and exact
final-head CI required before readiness. Collections/dispatch/general exception/
effect IR, extended signatures, compiled core/macros and production frontend
migration remain. Next extend closure signatures and prerequisite core/collection
foundations. No complete issue/milestone claimed; all M2–M9 remain unfinished.


## PR #63 independent recurrence review — 2026-09-30

A dispatched independent reviewer inspected head 51359f0 against reviewed base
e22b45d in /private/tmp/suss-review-pr63. Reviewed lexical target/tail analysis,
sequential initializer scope, parallel edge assignment, divergence-aware joins,
function isolation, dynamic header facts, old iteration/outer capture rooting,
public HIR target/arity checks and executable negative/reference coverage.
No significant production-code defect was found. Documentation corrections
reconcile fixed-function recur versus unsupported named self-recursion, current
primitive/core arithmetic and session evidence, and the M2-02/M2-03 issue manifest.
Stable issue IDs/statuses and all incomplete acceptance gates remain unchanged.

Independent sequential native checks used CARGO_BUILD_JOBS=2, shared
CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target and no RUSTFLAGS override:

- Six compiler suites: 72 passed, zero ignored, exit 0;
  /private/tmp/suss-pr63-review-compiler.log.
- Native Session library/integration: 24 passed (four private/20 integration),
  zero ignored, exit 0; /private/tmp/suss-pr63-review-session.log.
- Required cargo test --workspace --locked -- --test-threads=2: passed, exit 0
  including CLI rustdoc; /private/tmp/suss-pr63-review-full.log. Existing legacy,
  manual and documentation ignores are retained; this is not full compatibility.
- Python regression suite: 54 passed, exit 0;
  /private/tmp/suss-pr63-review-python.log.
- Fresh clean pinned ClojureScript c4295f303100bbf5afac449242d30bca1126f1a1,
  generated with oracle_cases.py/portable_oracle.py, compiled by cljs.main and
  executed in Node: 194 observations match exactly, exit 0;
  /private/tmp/suss-pr63-review-oracle-build.log and
  /private/tmp/suss-pr63-review-oracle-comparison.log. Pipeline tests independently
  execute/decode every corpus artifact. Upstream arithmetic warnings remain visible.
- Inventory: 1,065 exact declarations/four in-progress reviews/1,061 unassessed.
  Pinned WIT source lock, numeric artifact/source fingerprints, offline roadmap
  preview and git diff --check pass. Initial review utility invocations used
  incorrect relative/script paths and a nonexistent wasi_lock --check option;
  corrected to actual scripts and wasi_lock.verify. No failed invocation is
  interpreted as success and no source/runtime repair was needed.

All native and reference handles are terminal. Require successful CI on the exact
final reviewed head including these documentation changes before readiness.
PR #63 remains unmerged and uses Refs #9/#10, because collections/dispatch/general
exception/effect IR, extended signatures, source-backed core/macros and production
frontend migration still prevent full acceptance. No milestone closes here.

Next unblocked task: extend closure signatures with full active-signature targets
and callable identities, while preserving ABI v1 and exact captures. Root separately
compiled and strict-tag validated 11 extended-signature reference-only cases:
/private/tmp/suss-extended-signature-probe-inputs.json and -observations.json,
/private/tmp/suss-extended-signature-reference-fixed.log. An initial namespace-path
fixture failure remains in -reference.log. The successor plan is
/private/tmp/suss-signatures-next-plan.md. These are not Suss compatibility passes
and do not enlarge this PR's recurrence scope. M2–M9 remain unfinished.

## M2-03 acceptance reconciliation — 2026-09-30

A read-only follow-up independent audit of issue #10 against its actual four
criteria found no remaining foundation requirement: cross-fragment scalar/closure/
UTF-16 roots survive GC, universal fixed/variadic invocation guards arity, exact
binary64/UTF-16 corpus passes (194 source/1024 numeric), and version/actual-layout
mismatches fail before initializer effects. Broader signatures/core/macros/nominal
machinery/frontends belong to #9/#11/M3–M7. Earlier blanket incomplete notes mixed
work-package boundaries; do not keep a fulfilled issue open for unrelated future
work. Root verified the named tests and native install gate ordering against code.

Isolated /private/tmp/suss-abi-v1-acceptance, resurrection/abi-v1-acceptance, is based
on reviewed7912975. docs/roadmap/acceptance-runtime-abi-v1.md maps every criterion
to executable tests, prior full/fresh observations and remaining separate work.
ROADMAP and stable M2-03 manifest status now reflect completion on this branch;
GitHub closure and default-branch completion await merge of its Closes #10 PR.
M2 overall and #9 remain incomplete; other IDs/statuses/dependencies unchanged.
Stale ABI closure/call/definition/native-loader statements reconciled. No runtime,
source implementation, ABI layout, dependency, upstream copy or license changed.

Exact runtime_abi focus: 10 pass, zero ignored, exit0;
/private/tmp/suss-abi-v1-acceptance-focused.log. Command cargo test -p suss-compile
--test runtime_abi --locked -- --test-threads=2, CARGO_BUILD_JOBS=2/sharedtarget,
no RUSTFLAGS override. Python54 pass, exit0;
/private/tmp/suss-abi-v1-acceptance-python.log. Offline publish_roadmap preview and
diff checks pass, with stable issue IDs. Root/independent full baseline and194fresh
observations from unchanged implementation are recorded above. Independent audit
PR review and exact final-head CI are required; do not merge or close manually.
PR63 final-head CI36686074513 is currently confirmed live; no pass claimed yet.
Next implement named/multiple closure signatures from the genuine isolated red
regression and fresh11reference observations, then broader foundations/M2–M9.

## PR #64 independent acceptance review — 2026-09-30

Dispatched reviewer /root/review_pr64 audited 77a436a against reviewed PR #63
7912975 in isolated /private/tmp/suss-review-pr64. Read the accepted design,
roadmap, inventory and handoff, the remote issue #10's four published criteria,
runtime construction/invocation/gate code and the named executing regressions.
All four foundation criteria are supported; no unresolved significant production
or acceptance defect was found. M2 overall and #9/#11 remain incomplete, and
branch completion is explicitly distinguished from unmerged default-branch state.
Stable work-package IDs and dependencies are unchanged.

Corrected an evidence overstatement: the native private installation regression
checks malformed artifacts and missing cell imports, whereas actual changed-layout
rejection before effects is proved by the runtime host-marker test. Native install
verifies/validates before staging cells and publishes only after linking all
fragments. No runtime/code/ABI change was needed.

Independent sequential checks with CARGO_BUILD_JOBS=2/shared CARGO_TARGET_DIR and
no RUSTFLAGS override passed, all exit 0 and zero ignored in focused native suites:
runtime_abi 10 (/private/tmp/suss-pr64-review-runtime.log); portable_pipeline 16,
including execution/independent decoding of all 194 corpus fragments
(/private/tmp/suss-pr64-review-pipeline.log); native artifact-install gate 1
(/private/tmp/suss-pr64-review-native-gate.log); old source closures/owned UTF-16
across rebinding and GC 1 (/private/tmp/suss-pr64-review-native-roots.log); Python54
(/private/tmp/suss-pr64-review-python.log). Offline roadmap preview reports stable
10 milestones/39 issues; diff checks pass. Reviewed prior unchanged-code full
baseline and fresh194 reference comparison evidence; no unnecessary full rerun
for this documentation-only audit. Existing legacy/manual ignores remain explicit.
All native handles are terminal. Exact final acceptance-head CI is still required.

Verified GitHub's official closing-keyword rules: descriptions only auto-close
when targeting the default branch; commit keywords close when the commit reaches
that branch. Audit and PR template now explain predecessor landing/retargeting and
preserving closing keywords on squash. This review commit includes Closes #10,
because all its actual criteria pass; no other incomplete issue is closed.
Do not merge or manually close. Next unblocked implementation remains extended
source signatures and broader M2–M9 acceptance.

## Extended closure signatures: pre-implementation regression — 2026-09-30

Isolated /private/tmp/suss-portable-signatures on resurrection/portable-signatures
is based on PR #64's final independently reviewed d52b7404ccbfc3f1a5b624ea1df756ef6dfd17cc.
PR #63 reviewed-head CI36686074513 passed on exact7912975; root inspected actual executing recurrence/source tests and CLI rustdoc. PR #64 exact final-head CI36687560807 passed on d52b740; root verified named runtime/source/native gates and marked the unmerged PR ready. Issue10 stays open until default-branch merge.
Added native source regression for multiple fixed signatures, per-signature recur,
outer/nested captures, named functions and direct/higher-order self-reference.
Exact focused command CARGO_BUILD_JOBS=2 with shared CARGO_TARGET_DIR,
cargo test -p suss-cli --test persistent_session
persistent_session_named_and_multiple_fixed_signatures --locked -- --test-threads=2
fails exit101 with located bytes5..12 "Named/multiple-arity functions are not lowered
yet; expected parameter vector". /private/tmp/suss-portable-signatures-red.log.
Native handle76303 is terminal. No source implementation or successor PR yet;
the regression remains red and must not be skipped. Eleven fresh pinned scalar
observations are reference-only as above. The next implementation must preserve
active-signature recur scope, true named callable identity, captures/GC/rebinding
and arity gaps with typed language diagnostics; raw array rest values cannot stand
in for portable sequences. All broader acceptance remains unfinished.

## Named/multiple fixed signatures: implementation validation — 2026-09-30

GeneralFunction HIR retains independent methods and source-aware named binding;
MakeGeneralClosure IR verifies shared outer captures, optional self slot and method
entry shape/arity. Generated shared Invoke dispatcher reads argument length once
and routes exact methods; arity-error is an additive private runtime export using
the central descriptor/tag/message. Named construction initializes the reserved
self slot before publication; immutable outer captures remain unchanged. No ABI
layout/version change, helper artifact/dependency/license change or copied source.
Semantic provenance and limits are in docs/runtime/closure-signatures.md.

Original source red now passes all11named/multiple reference cases. A first compiler
invocation failed because local dispatcher index shadowed the import lookup closure;
renamed to dispatcher_function and preserved /private/tmp/suss-portable-signatures-first.log.
Corrected regression passes, -first-fixed.log. Fresh six-case reference probe shows
last-body duplicate-arity behavior (upstream warnings retained), parameter shadowing,
empty body and exact self identity; /private/tmp/suss-signature-edges-observations.json
and -reference.log. Normalization retains the last signature and recomputes captures.
Five scalar edge cases join eleven prior cases in the common210source corpus;
identity is independently inspected in native tests, not forged into scalar success.

New native tests prove exact self reference identity after GC, nested self capture,
old named functions across global replacement, duplicate/shadow behavior and arity
gaps with source-order effects retained and no body effects. Public IR mutation
tests reject missing methods/duplicates/capture/self corruption/usize::MAX arity;
source negative tests isolate active-signature recur and tail scopes. Old unsupported
name/multiple assertions are replaced by executing coverage; variadic/destructuring/
condition restrictions remain explicit. No skips or legacy failures hidden.

Sequential native graphs use CARGO_BUILD_JOBS=2/shared CARGO_TARGET_DIR and no
RUSTFLAGS override. Compiler six-suite focus73passes/zeroignored, exit0,
/private/tmp/suss-portable-signatures-compiler-final.log. Session27passes (23integration/
fourprivate)/zeroignored, exit0, -session-final.log. Fresh pinned oracle210exact
observations plus16actual decoded pipeline tests pass, exit0, -oracle.log. Python54
passes, -python.log. Inventory1065/fourin-progress/1061unassessed, WIT15/sixpackages,
numeric integrity, offline roadmap preview and touched formatting/diff checks pass.
Required full workspace baseline passed, exit0 including CLI rustdoc;
/private/tmp/suss-portable-signatures-full.log. Nativehandle75972 is terminal;
all source/reference/native processes are terminal and no graphs overlapped.

Next publish stacked on reviewed/CIpassingPR64d52b740 with Refs #9/#10, independent
subagent review/pushed significant fixes and exact final-headCI before readiness.
Variadic rest requires real portable persistent sequences rather than argument
arrays; destructuring/conditions, core/macros, collections/nominal dispatch/general
exception/effect/async IR and production frontend acceptance remain. No future
milestone or inventory completion claimed. Next build persistent sequence/core
foundations for full signatures and #11/runtime nominal machinery. M2–M9 unfinished.

## Independent PR #65 review — 2026-09-30

Reviewed 90bab7e independently in /private/tmp/suss-review-pr65, based on the
reviewed PR #64 d52b740. Read the accepted design, roadmap, inventory and handoff;
audited signature normalization, lexical self identity/shadowing, selected-method
captures, isolated recur targets, public IR verification, shared Invoke dispatch,
arity gaps and cyclic environment initialization before publication. No significant
production defect was found. Linking resolves the additive arity-error import
before any fragment eval; the ABI recursive layout/version remains unchanged.
Corrected stale PROVENANCE.md wording: the overlay has four adapted in-progress
arithmetic reviews and 1,061 unassessed declarations, with no completed upstream
form ports. No source implementation, dependency or artifact changed in review.

Independent sequential checks used CARGO_BUILD_JOBS=2 and shared CARGO_TARGET_DIR,
without a RUSTFLAGS override. Focused portable_closures 13, portable_pipeline 16
and runtime_abi 10 passed, exit 0, zero ignored;
/private/tmp/suss-pr65-review-focused-fixed.log. An initial command incorrectly
named nonexistent portable_recur and failed before running tests, exit 101;
/private/tmp/suss-pr65-review-focused.log. It was corrected rather than skipped.
Python54 passed, exit 0, /private/tmp/suss-pr65-review-python.log. Required full
cargo test --workspace --locked -- --test-threads=2 passed, exit 0 including CLI
rustdoc and all 27 session tests, /private/tmp/suss-pr65-review-full.log. Existing
legacy/manual ignores remain explicit. Fresh pinned source oracle210 observations
matched exactly and all16 actual decoded pipeline tests passed, exit 0;
/private/tmp/suss-pr65-review-oracle.log. cljs_reviews.py verifies 4/1061; diff
checks pass. Every review process is terminal, with no overlapping Cargo graphs.

Push this review evidence to PR #65, then require successful CI on that exact
final head before readiness. Refs #9/#10 only; do not merge or close incomplete
issues. Next unblocked work remains portable persistent sequence/core foundations,
rest signatures and broader nominal machinery; M2–M9 are unfinished.


## Reviewed signature readiness and next nominal source boundary — 2026-09-30

PR #65 final-head CI36691007009 passed on independently reviewed
838ea406806572eb580f853c18787bb4ec9ac32b. Root verified actual full workspace,
native session, executing pipeline/ABI and CLI rustdoc results in
/private/tmp/suss-pr65-reviewed-ci.log. PR marked ready; no merge performed.
Predecessor36690325724 was cancelled, not passed. Refs #9/#10 only; PR #64
contains the separate completed runtime foundation audit and Closes #10 linkage.

Next worktree /private/tmp/suss-portable-nominal starts at that reviewed head.
Two source-level native regressions currently fail: distinct same-layout deftype
identity through aliases/GC and live protocol extension across fragments.
CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2
cargo test -p suss-cli --test persistent_session nominal_ --locked -- --test-threads=2
exited101 with two failures/zero passes: located unresolved deftype bytes1..8 and
defprotocol bytes1..12. /private/tmp/suss-portable-nominal-red.log records this
actual unsupported boundary. No implementation or successor PR yet.

Pinned development ClojureScript/Node produced eleven strictly validated reference
observations for distinct nominal identity, constructor aliases and extension of
existing objects; /private/tmp/suss-nominal-reference-observations.json. This is
reference evidence only, not Suss compatibility. Initial probe compilation failed
with unmatched delimiter, then Node failed because no output existed; retained
/private/tmp/suss-nominal-reference-probe.log. Corrected probe exited0,
/private/tmp/suss-nominal-reference-probe-fixed.log. All native/reference processes
are terminal. Next implement real descriptor identity/protocol dispatch in the
accepted shared prelude, then execute these red tests; preserve #11's full
exception/dynamic-scope acceptance criteria. M2–M9 remain unfinished.


## Nominal runtime implementation before source lowering — 2026-09-30

Added original runtime_abi/nominal.rs: checked descriptor/schema/field storage,
nominal matching by rooted descriptor reference, live per-descriptor method table,
constructor and protocol dispatcher shared Invoke factories, and typed nominal
error descriptor7. Numeric diagnostic identities start8; exhaustion rejects before
wrapping. New counter/error globals append after numeric stack global6; existing
helper bytes/dependencies, ten-type prelude and ABIversion1 remain unchanged.
Schemas/fields are copied; table replacement preserves other entries; growth is
published only after copying; existing objects/dispatchers see live method updates
while an old captured method retains its environment. Protocol dispatcher
environments use an internal one-key array, so constructor-descriptor rejects
them. No internal array is claimed as a portable persistent collection.

Three executing runtime regressions cover actual independently generated
fragments/GC, same-layout distinct identities, forged equal numeric identities,
constructor Invoke, existing/captured protocol dispatch, old method behavior,
other entries, typed bad schema/table/field/receiver/arity and recovery. Full
selected compiler/runtime gate76 passed: closures13, definitions12, modules11,
pipeline16, resolution11, runtime13 (including numeric1024). Exact command used
CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2
cargo test -p suss-compile --test runtime_abi --test portable_closures
--test portable_pipeline --test portable_definitions --test portable_modules
--test portable_resolution --locked -- --test-threads=2; exit0, zero ignores,
/private/tmp/suss-nominal-compiler-runtime-final.log. An intermediate test compile
failed with nonexistent AnyRef::ref_eq; corrected to Rooted::ref_eq, not skipped,
/private/tmp/suss-nominal-runtime-callables-focused.log and -focused-fixed.log.

Native current-head command cargo test -p suss-cli --lib --test persistent_session
--locked -- --test-threads=2 (same target/two jobs) exited101 as expected: private4
and existing integration23 passed, two new source regressions failed at unresolved
deftype1..8 and defprotocol1..12. No native pass, full baseline or source support
is claimed for those red cases. /private/tmp/suss-nominal-native-boundary-final.log.
Python54 and offline roadmap preview passed, with diff checks clean. Logs
/private/tmp/suss-nominal-python.log and -roadmap-preview.json. Native graph handles
are terminal and did not overlap. Initial broad fmt touched unrelated files;
those formatting-only changes were restored from HEAD, preserving new tests/code.

These are uncommitted implementation changes in /private/tmp/suss-portable-nominal;
no successor PR, independent review or final-head CI exists yet. Do not publish
readiness or close #11. Next lower genuine type declarations/constructors and
protocol declarations/extensions through verified HIR/IR, preserving evaluation
order, exact phase/name identities and compile-error atomicity; execute the native
source regressions, broaden fresh pinned scalar evidence, then full required
workspace baseline, PR review/fixes and exact-head CI. Built-in/protocol-wide
dispatch, source field mutability/metadata, exceptions and dynamic scope remain
in #11; M2–M9 are incomplete. No upstream core form copied or certified.


Additional pinned frontend probe changes the next lowering decision: a deftype
expression is truthy, fn? on its constructor is true, dynamic (new klass 7) works,
but ordinary (NominalCall 7) returns nil (compiler warning retained), not a new
instance. Four reference-only observations strictly validated;
/private/tmp/suss-nominal-frontend-reference.json and -reference-fixed.log. Initial
probe failed on an unmatched delimiter; /private/tmp/suss-nominal-frontend-reference.log
retained, corrected probe exited0. All processes now terminal. Do not bind a source
type directly to the private constructor-new callable and accidentally change
ordinary invocation into object construction. Separate class value/ordinary-call
behavior from new/dotted constructor lowering; preserve aliases and source operand
order. Inspect/probe other arities before claiming full constructor semantics.


## Nominal source pipeline and fresh reference evidence — 2026-09-30

The formerly red source boundary now lowers bounded deftype, defprotocol,
extend-type, new/dotted construction, instance? and satisfies? through verified
HIR/IR and the shared runtime. Current behavior and explicit limits are recorded
in docs/runtime/nominal.md. Descriptor references establish identity, canonical
phase/name/method/arity keys survive protocol redeclaration, and live extensions
reach existing objects/captured dispatchers across fragments and forced GC.
Generated arrows consult the current type cell while captured class values retain
the original descriptor. Implicit method fields and physical receiver retain the
original object across recur; ignored receiver replacements still evaluate.
Compiler-owned key cells are inaccessible to source and compile errors publish
no staged bindings. Empty extensions return the rooted opaque protocol sentinel.

Fresh probes exposed two important incorrect assumptions and were repaired:
source constructor calls permit missing/extra fields, and ordinary type calls
produce undefined, whose coercions differ from nil. Internal i31 sentinel6 now
preserves falseyness, NaN number conversion and string “undefined” conversion;
missing fields use it. The original nine-test native run had one actual failure
before repair (/private/tmp/suss-nominal-undefined-red-fixed.log). Compiler format
is now 0.1.0+portable.2; legacy compiler manifests reject before initializer
side effects. ABIversion1, the ten-type prelude, numeric stack global6, numeric
helper bytes and dependency lock are unchanged. Protocol values are callable
closures over private key bundles. Guarded native fallback rejects unsupported
user-object native tables instead of claiming a false result.

The initial 242-case pinned run failed on incorrectly grouped deftype overloads
(/private/tmp/suss-nominal-oracle.log); corrected inputs use separate method forms,
and grouped deftype signatures now have a located compile rejection regression.
Nothing was skipped or recategorized as a known failure. Fresh reference-only
probe files preserve constructor warnings, redeclaration/arrows, anchored method
receiver, opaque marker identity and downstream undefined coercions. Strict tagged
validation checked protocol5, arrow10, method-recur3 and undefined6 observations;
these development-only probes are reference evidence, not Suss compatibility.

Current selected compiler/runtime gate78 passed with no ignored tests:
CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2
cargo test -p suss-compile --test runtime_abi --test portable_closures
--test portable_pipeline --test portable_definitions --test portable_modules
--test portable_resolution --test portable_nominal --locked -- --test-threads=2.
Log /private/tmp/suss-nominal-current-compiler.log, exit0. Native private4 and
integration32 (including nine nominal tests) passed on the latest source/runtime:
cargo test -p suss-cli --lib --test persistent_session --locked -- --test-threads=2
with the same target/two jobs; /private/tmp/suss-nominal-current-session.log exit0.
Membership also executes after replacing the protocol var with nil: canonical
marker fast path succeeds, fallback fails as a language error and prompt recovers.

CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2
sh scripts/test-portable-pipeline-oracle.sh exited0 on the current implementation:
all242 fresh pinned Node observations match and all16 independently decoded
portable pipeline tests pass; /private/tmp/suss-nominal-current-oracle.log.
Python54 passed (/private/tmp/suss-nominal-current-python.log). Review validation
reports9 in-progress reviews/1056 unassessed; the five nominal macro adaptations
retain exact source hashes, dependency/evidence references and EPL provenance.
No form is marked implemented. Offline roadmap preview and diff checks passed.
The legacy16-case source corpus remains9 passing/7 exact failures/0 skipped.

The required full locked workspace baseline is running separately, with no native
Cargo graph overlap, /private/tmp/suss-nominal-current-workspace.log. Its result
must be recorded before publishing readiness. Changes remain uncommitted and no
nominal PR/review/final-head CI exists yet. Next complete the full baseline, commit
and push resurrection/portable-nominal, open a stacked PR against #65 with Refs #9
and Refs #11, dispatch its independent reviewer, push significant fixes and require
exact final-head CI. Do not close #11 or merge any PR. Builtin/native/wildcard
protocol dispatch, field attributes/metadata, full source core/compiled macros,
exceptions and dynamic scope remain necessary work; M2–M9 are incomplete.


Required full workspace result: CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target
CARGO_BUILD_JOBS=2 cargo test --workspace --locked -- --test-threads=2 exited0;
/private/tmp/suss-nominal-current-workspace.log. Includes private session4,
integration32, compiler unit54, legacy expressions317/12 existing ignores,
components29, conformance9/2 manual ignores, oracle4/1 manual capture ignored,
portable nominal2/runtime13 and all remaining suites and CLI/doc-test targets.
Existing ignores and the legacy source differential failures are unchanged.
This verifies the local candidate, not independent review or final-head CI.

## Independent PR #66 review and semantic correction — 2026-09-30

The preceding uncommitted/publication statements describe earlier snapshots.
Candidate d0efeb129a56bb5683604017162368277e956ecf was committed and published as
stacked draft PR #66 against reviewed PR #65, with Refs #9 and Refs #11.
Independently reviewed in /private/tmp/suss-review-pr66: accepted design, roadmap,
inventory/handoff, source bootstrap, lexical/phase identities, canonical key cells,
HIR/IR shape and source ordering, descriptor reference identity, guarded tables,
constructor normalization, receiver anchoring, live dispatch and publication.
No merge or incomplete issue closure is authorized.

Significant finding: defprotocol's expression result was incorrectly source nil.
Fresh pinned execution shows undefined: adding 1 yields NaN, and adding "x"
yields "undefinedx", rather than 1/"nullx". Existing falseyness and nil-like tagged
transport missed this distinction. The new native regression failed exit101 before
repair (/private/tmp/suss-pr66-review-defprotocol-red.log). Added internal HIR
Undefined literal, typed Value, emitted as the existing i31 sentinel6; no source
undefined literal or ABI/compiler-format change. Defprotocol now returns that value.
Corrected focused native test passes, -defprotocol-fixed.log. A stale comment about
missing constructor fields becoming nil was also corrected to undefined.

Three shared cases add both coercions and grouped extend-type overloads, the latter
explicitly supported by pinned core.cljc1668–1708 and executed successfully in Node
and the native session over an existing object after GC. Corpus now245, with35
nominal cases; ten native nominal tests bring session focus to37 (33 integration,
four private). Current counts/docs updated, historical evidence left intact.
First oracle invocation failed before execution because review-authored expected
number tags were incorrectly named "number"; strict transport rejected them,
/private/tmp/suss-pr66-review-oracle.log. Correcting to f64 exposed the actual
reference mismatch above, -oracle-fixed.log. No failure was skipped or hidden.

Independent sequential commands used CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target
and CARGO_BUILD_JOBS=2, without RUSTFLAGS overrides. Fresh sh
scripts/test-portable-pipeline-oracle.sh passed245 exact pinned observations and16
actual decoded fragment tests, exit0, /private/tmp/suss-pr66-review-oracle-final.log.
Compiler seven-suite focus78 passed, exit0, -compiler.log; native --lib and
persistent_session37 passed, exit0, -session.log. Python54 passed, -python.log;
cljs_inventory.py --check verifies1065 and cljs_reviews.py verifies9 in-progress
reviews/1056 unassessed. Diff checks pass. Required full workspace baseline follows
before pushing review changes; readiness still requires exact final-head CI.

Builtin/native/wildcard tables, field attributes/metadata, complete core/compiled
macros, general exceptions and dynamic scope remain unfinished. Those limits are
explicit rather than inferred success. Next finish broader #11 exception/dynamic
scope and nominal acceptance, persistent sequence foundations and full #9 source
signatures; M2–M9 remain incomplete. No new significant findings remain in the
bounded implementation reviewed here; this does not certify full protocol/core
compatibility.

Independent required cargo test --workspace --locked -- --test-threads=2 passed,
exit0, /private/tmp/suss-pr66-review-workspace.log, including all session tests,
executing compiler/runtime suites and CLI rustdoc. Existing legacy/manual ignores
remain explicit and unchanged. The final sign-neutral NaN regression assertion
also passed its focused rerun, exit0, -final-regression.log; production code was
unchanged after the full baseline. Every review native/reference graph is terminal
and no Cargo graphs overlapped. Push this reviewed fix/evidence, then require
successful CI on that exact final head before PR readiness. Do not merge or close
issues #9/#11.


## Portable exceptions acceptance preparation — 2026-09-30

A separate /private/tmp/suss-portable-exceptions worktree/branch now prepares
remaining issue11 exception acceptance, initially based on PR66 candidate d0efeb1, now synchronized to independent review fix426e7a272f88204d55472c661e9765b9416bc9f5.
Six new executing integration regressions in crates/suss-cli/tests/portable_exceptions.rs
cover exact thrown nil/false/binary64/surrogate values and prompt recovery; body/catch
results with finally cleanup and superseding throws; same-layout nominal typed catch
identity with retained payload roots; captured/shadowed catch locals across GC;
located malformed handler/cross-region recurrence rejection without publication;
and runtime arity errors leaving old definition cells unchanged despite prior effects.
These tests are not executed yet: review_pr66 exclusively holds the shared native
Cargo target while verifying its significant defprotocol undefined-return repair.
No exception implementation, test pass, PR or issue closure is claimed.

Primary pinned analyzer.cljc1891–1994 confirms exact single-operand throw,
ordered nominal instance? catch selection/default rethrow, disallowed recurrence
across try/catch/finally, and expression/statement handler contexts. An original
reference-only fixture executed14 tagged exception observations in the pinned
ClojureScript/Node oracle; strict ID/schema/value validation and exact scalar
expectations passed. /private/tmp/suss-exception-reference-inputs.json and
/private/tmp/suss-exception-reference-observations.json preserve observations.
Body42 survives finally7; catch/rethrow retains9; finally42 supersedes body/catch
throws; two same-layout catch types select the actual descriptor; nested side
effects produce1234; negative zero/lone surrogate/nil/false payloads remain exact.
Empty try is nil-like and captured local35 plus caught7 returns42. Initial fixture
creation used a wrong relative path and the compiler failed; retained
/private/tmp/suss-exception-reference.log. Corrected compile exited0 with ordinary
var replacement warnings preserved, /private/tmp/suss-exception-reference-fixed.log;
Node/strict validation exited0. This is reference evidence only, not Suss success.

Next synchronize the exception branch to the final independently reviewed nominal
head, execute these regressions to establish the actual red boundary, and implement
explicit throw/handler control flow in verified HIR/IR and Wasm typed exception tags.
Preserve operand order, exact payloads, catch scope/captures, compile atomicity and
finally behavior on normal return/body throw/catch throw/cleanup throw. Dynamic
bindings and ExceptionInfo/core surfaces remain required issue11 work; no milestone
is complete. Nominal PR66 remains draft pending review fixes and exact final-head CI.


Exception boundary execution after review release: next branch now actually
rebases on origin/resurrection/portable-nominal426e7a272f88204d55472c661e9765b9416bc9f5
(the local nominal ref initially still pointed at d0efeb1; authoritative remote
tracking ref and subsequent rebase verified the final head). First test build
failed from returning a borrowed Rooted through inspect; repaired the test to use
to_owned_rooted, preserving the diagnostic log /private/tmp/suss-portable-exceptions-red.log.
Then CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2
cargo test -p suss-cli --test portable_exceptions --locked -- --test-threads=2
exited101 with all6 regressions failing, zero ignored. This is the intended actual
red source boundary, not a compiler build failure: try remains unresolved at
located spans1..4/20..23 and throw does not yet produce a language payload.
/private/tmp/suss-portable-exceptions-red-fixed.log. All native/reference handles
are terminal and no graphs overlapped. No passing exception acceptance claimed.

PR66 independent review has pushed its significant defprotocol result fix and
verified local full workspace, fresh245 source observations/16 actual pipeline,
compiler78/native37/Python54. Root synchronized the local nominal branch and PR
body to that exact reviewed head. Final-head CI36707481991 is confirmed running
on426e7a272f88204d55472c661e9765b9416bc9f5; do not mark ready until its actual
success. No issue or PR was closed/merged. Next implement the complete exception
control flow against these actual red regressions; source dynamic scope and
ExceptionInfo plus broader nominal/core boundaries remain issue11 obligations.


## Source throw/try implementation and nominal PR readiness — 2026-09-30

PR66 is now ready, independently reviewed426e7a272f88204d55472c661e9765b9416bc9f5,
exact CI36707481991success verified through current PR head and actual workflow
logs (/private/tmp/suss-pr66-final-ci.log) including source fragments/native/full
workspace/doc targets. Body and issue9/11 comments record significant defprotocol
fix and current245-source/37-native evidence; readiness comments5910311861 (11)
and5910312205 (9). No PR merged or incomplete issue closed.

The isolated exception worktree now implements original HIR Throw/Try and verified
IR terminal Throw edges/Try regions. Compiled body/handler/cleanup closures use the
existing shared Invoke ABI; runtime try-invoke uses nested typed language regions
so cleanup runs once on normal/body/catch paths and cleanup exceptions supersede
pending payloads. Typed source catches evaluate nominal tests in order, default is
last, exact payloads and catch captures survive GC. Divergent operands stop later
effects/publication; defonce retains its bound path when the initializer path
throws. Region recurrence cannot target outside loops/functions, while inner
loops/functions remain legal. Manifestformat2, runtimeABI1, prelude, numeric
helper/dependencies are unchanged. See docs/runtime/exceptions.md for boundaries.

First implementation build failed on sibling helper visibility and Catch enum/name
collision; corrected and retained /private/tmp/suss-portable-exceptions-first.log.
The actual next run passed5/6, with the final regression exposing a test's statically
known wrong arity (compile error, not runtime throw). Repaired its callee to a live
cell, retaining /private/tmp/suss-portable-exceptions-first-fixed.log; all6 then
passed /private/tmp/suss-portable-exceptions-source-fixed.log. Added seventh ordered
divergence/defonce/loop/callee/nested-throw regression; native private4+integration33
+exception7 passed44, /private/tmp/suss-portable-exceptions-native-expanded.log.
No failures were skipped, masked or declared successful.

Public exception guards first failed because the test assumed a loop body omitted
its retained Do wrapper, /private/tmp/suss-portable-exceptions-compiler.log. Corrected
actual-tree traversal, then compiler eight-suite80 passed with zero ignores:
CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2
cargo test -p suss-compile --test runtime_abi --test portable_closures
--test portable_pipeline --test portable_definitions --test portable_modules
--test portable_resolution --test portable_nominal --test portable_exceptions
--locked -- --test-threads=2; /private/tmp/suss-portable-exceptions-compiler-fixed.log.

Fresh current sh scripts/test-portable-pipeline-oracle.sh with the same target/two
jobs exited0:259 pinned source observations match and16 actual decoded pipeline
tests pass, /private/tmp/suss-portable-exceptions-oracle.log. Fourteen new exception
inputs reuse strictly validated primary observations; original245 inputs unchanged.
Python54/inventory1065/reviews9 in-progress+1056 unassessed/offline roadmap preview
and diff checks pass. No core form copied or certified; pinned analyzer.cljc provenance
is development-only. All focused/reference processes are terminal.

Full required cargo test --workspace --locked -- --test-threads=2 is running alone,
/private/tmp/suss-portable-exceptions-workspace.log, with the shared target/two jobs.
Changes remain uncommitted, no exception PR/review/final-head CI exists yet. Next
verify full baseline, commit/push a stacked exception PR against ready #66 using
Refs #9/#11, dispatch independent review, push significant fixes and require exact
CI. Then implement dynamic scope/ExceptionInfo and broader nominal/core requirements;
#11 and M2–M9 remain incomplete. Fuel traps/foreign host tags remain distinct;
portable language handlers do not silently turn them into nil values.


Required exception candidate full baseline now terminal exit0:
CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2
cargo test --workspace --locked -- --test-threads=2,
/private/tmp/suss-portable-exceptions-workspace.log. Includes native44, compiler
unit54, legacy expressions317/12 existing ignores, components29, conformance9/2
manual ignores, oracle4/1 manual capture ignored, portable compiler80 including
259-case actual source artifacts, reader/core suites and all doc targets including
suss_cli. Existing ignores/legacy9pass7fail differential baseline unchanged.
No native/reference handle remains live. Independent review/fixes and exact final-head
CI remain required for the next PR. No issue closure or merge.


## Independent PR67 exception review — 2026-09-30

Reviewed candidate a83bffd700101984bab26e2ac9d40156481e07b2 against accepted
suss-0.3.1, roadmap/inventory and pinned analyzer.cljc1891–1994 in isolated
/private/tmp/suss-review-pr67. No significant production defect found. Added three
executing regressions: unhandled nil/closure payloads with exactly-once cleanup,
closure invocation after GC; lazy source-ordered computed nominal catch operands,
skipped tests after match and a throwing catch test superseded through cleanup;
and nested field/loop captures across exception regions. All10 exception integration
regressions pass, including the candidate's original7. These additions increase
native session gates to47; public exception shape/recurrence guards remain2.

Focused cargo test -p suss-cli --test portable_exceptions -p suss-compile
--test portable_exceptions --locked -- --test-threads=2 passed9 original guards/
source tests; expanded suss-cli target passed10. Required cargo test --workspace
--locked -- --test-threads=2 passed including47 native gates, compiler80 selected
portable suites, 259-source actual artifacts, legacy317/12 existing ignores,
components29 and all doc targets including suss_cli. Logs:
/private/tmp/suss-review67-focused.log, /private/tmp/suss-review67-expanded.log,
/private/tmp/suss-review67-workspace.log. Every native invocation used
CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2;
no RUSTFLAGS override and no overlapping Cargo graphs.

Fresh sh scripts/test-portable-pipeline-oracle.sh then passed259 pinned observations
and16 independently decoded actual pipeline tests; /private/tmp/suss-review67-oracle.log.
No source corpus input changed. Python54 and review overlay9/1056 pass. Initial
review overlay failed before the isolated pinned submodule was initialized; fixed
by initializing exact c4295f303100bbf5afac449242d30bca1126f1a1. Initial Python
invocation used a nonexistent scripts/tests directory; retained failures in
/private/tmp/suss-review67-python.log and -python-fixed.log; corrected
python3 -m unittest discover -s scripts -p 'test_*.py' passed54, -python-corrected.log.
No failing semantic result was hidden or skipped. All process handles terminal.

Push review regressions/evidence to PR67, then require CI on the exact final pushed
head before readiness. No merge or issue closure. Dynamic binding, ExceptionInfo/
public Error surfaces and broader nominal/core criteria remain issue11 obligations;
source collections, compiled macros, production migration and M2–M9 remain open.

## Dynamic binding work in progress — 2026-09-30

Worktree /private/tmp/suss-portable-dynamic-bindings, branch
resurrection/portable-dynamic-bindings, based on independently reviewed PR67 head
5ea35107a42099cc7289060549acc6214a54c3eb. PR67 exact-head CI36710520777
passed; root inspected actual full workflow log /private/tmp/suss-pr67-final-ci.log.
PR67 readiness comments5910927247 (#11)/5910927737 (#9). No merge or issue closure.

Original rooted Frame9/Cell5 scope lowering now implements binding/with-redefs,
global set!, accepted dynamic name metadata and parallel snapshot/publication order.
Escaped functions read current scope; lexical captures retain values. Body/cleanup
closures preserve typed throw/finally behavior. Compiler guards enforce complete
snapshot/value pairs and fixed body arity. Native eval/invoke/inspect retain caller
frame checkpoints; fuel traps remain traps and recovery runs private runtime pops,
not source finally callbacks. Full macro warnings/import, async context, ExceptionInfo,
public Error classes and broader issues9/11 remain incomplete. See dynamic-bindings.md.

Primary pin valid19 dynamic observations compiled and executed through Node:
/private/tmp/suss-dynamic-reference-valid.log. Exact schema/IDs/binary64 values
validated against /private/tmp/suss-dynamic-reference-observations.json. Original20
probe failed on local assignment (correct pinned rejection), retained separately
/private/tmp/suss-dynamic-reference.log. Non-dynamic-var warnings are preserved;
the bootstrap permits behavior but does not emit equivalent warnings yet.
Fresh sh scripts/test-portable-pipeline-oracle.sh passed278 source observations and
16 actual decoded pipeline tests before final pop hardening:
/private/tmp/suss-dynamic-oracle.log. Prior259 source inputs unchanged.

Native first6 regressions failed before implementation, -native-red.log; all6
subsequently passed -native-first.log. Actual fuel regression failed with leaked7
instead of1, -fuel-red.log; checkpoint restoration passed native54,
-native-guard.log. Compiler focus first failed on obsolete unsupported ^:dynamic
expectation, -compiler.log; removed that now-supported negative case, retained
executing metadata/scope tests. Nine selected compiler suites82 then passed,
-compiler-fixed.log. All logs use prefix /private/tmp/suss-dynamic.

A new 1..1200 fuel sweep actually failed at fuel430: body had entered, but root2
survived instead of pre-initializer snapshot1, -pop-fuel-red.log. Fixed pop to write
through an explicit parent context and publish parent only after restoration ends.
The child stays rooted, allowing interrupted reverse snapshot writes to be retried
idempotently. First refactor attempt missed helper local-index adjustments and
failed six cases, -pop-fuel-fixed.log; corrected actual indices. All8 dynamic tests
including sweep now pass, -pop-fuel-fixed2.log. No failures skipped or called green.

Two manual macro reviews added with exact pinned hashes, 11 in-progress/1054
unassessed; Python54 passed, -reviews.log/-python.log. No upstream form copied,
no dependency/lock/helper byte changes or Java/Node shipped runtime introduced.
ABI1 and compiler manifestformat2 unchanged. Current native full focus is running
alone, /private/tmp/suss-dynamic-native-final.log; includes new inspect callback
trap restoration regression. Next verify its actual outcome, rerun compiler/source
focus after hardening, then required full workspace baseline with shared target,
two build jobs and --test-threads=2. Commit/push stacked draft PR against #67 using
Refs #9/#11, dispatch independent review, push significant fixes and require exact
final-head CI. Changes remain uncommitted; no dynamic PR/review exists yet.

Final native focus terminal exit0, -native-final.log: private5 + persistent33 +
exception10 + dynamic8 =56 passed, zero ignored. Fresh final source rerun terminal
exit0, -oracle-final.log:278 pinned observations and16 actual decoded pipeline
tests pass after pop hardening. No native/reference process remains live.
Required full cargo test --workspace --locked -- --test-threads=2 now starts
alone, /private/tmp/suss-dynamic-workspace.log, using shared target/two build jobs.
Verify its actual terminal outcome before commit/push/review. No PR exists yet;
issues9/11 and M2–M9 remain incomplete.

Dynamic candidate required full baseline terminal exit0:
CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2
cargo test --workspace --locked -- --test-threads=2,
/private/tmp/suss-dynamic-workspace.log. Includes native56, selected portable
compiler82, actual278 source artifacts, runtimeABI13, legacy expressions317/12
existing ignores, components29, strict conformance9/2 manual ignores and all
reader/core/doc targets including suss_cli. Existing ignores and legacy
9pass/7fail differential baseline unchanged. No native/reference process live.

Committed implementation3937b7c58246f1f0c82292f9eece3231b26dc4c2 and pushed
resurrection/portable-dynamic-bindings. Draft PR #68 stacked on #67 uses
Refs #9/#11. Independent review and exact final-head CI remain required; no
readiness claim, issue closure or merge. Next reviewer inspects and pushes
significant fixes, then verify its actual results and final-head CI.

## Independent PR68 dynamic binding review — 2026-09-30

Reviewed candidate b87aefd3a2e37fb884f416790c031d63f04283aa against base
5ea35107a42099cc7289060549acc6214a54c3eb in isolated
/private/tmp/suss-review-pr68. Read accepted design, roadmap, compatibility inventory
and handoff, and inspected pinned core.cljc2301–2340. No significant production
finding in the bounded synchronous implementation. Checked parallel original/value
ordering, actual imported cell identity, copied frame entries, rooted parent/child
restoration, universal closure cleanup, typed payload ownership and native eval/
invoke/inspect checkpoint recovery. This does not certify async context, cancellation,
compiled macro/core import, warning policy or the remaining issues9/11 criteria.

Added independent regressions for nested multi-target/duplicate scopes interrupted
at every fuel1..2000, including initializer mutations and restartable reverse pop
writes; and escaping/thrown closure values retained through frame exit and GC.
All10 dynamic tests pass (candidate8 plus2), zero ignored, exit0:
CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2
cargo test -p suss-cli --test portable_dynamic_bindings --locked -- --test-threads=2,
/private/tmp/suss-review68-focused.log. Python54 and overlay11 reviewed/1054
unassessed pass, /private/tmp/suss-review68-python.log and -reviews.log.
Pinned submodule initialization confirmed exact c4295f303100bbf5afac449242d30bca1126f1a1.
Current docs now distinguish278 source cases and synchronous frame support from
historical259 exception evidence and pending async context. No inventory item
marked implemented; no upstream form copied or dependency/helper change.

Candidate CI36714342812 was running on b87aefd when root last checked;
predecessor3937b7c CI36714257887 cancelled, not a pass. Draft PR68 progress
comments5911235754 (#11) and5911236089 (#9) remain partial references.
The root candidate full baseline passed /private/tmp/suss-dynamic-workspace.log.
Independent full required workspace baseline and fresh source oracle follow;
readiness requires actual successful CI on the exact final pushed review head.
No merge or issue closure. Next after review/readiness continue ExceptionInfo,
public Error classes, builtin/native protocol/core requirements and source collections;
M2–M9 remain incomplete.

Independent required cargo test --workspace --locked -- --test-threads=2 passed,
exit0, /private/tmp/suss-review68-workspace.log. Includes native58 (private5,
persistent33, exception10, dynamic10), compiler82 selected portable suites,
278-source actual artifacts, legacy/component/reader/core suites and CLI doc target.
Existing manual/legacy ignores remain explicit and unchanged. Fresh independent
sh scripts/test-portable-pipeline-oracle.sh then passed278 pinned source observations
and16 actual decoded fragment tests, exit0, /private/tmp/suss-review68-oracle.log.
Both used shared target/two build jobs, no RUSTFLAGS override. All review process
handles12204/6955/21068/84178 are terminal; no native Cargo graphs overlapped.
Push review coverage/evidence, then require exact final-head CI before readiness.

## Stack #69 rebase onto current main — 2026-09-30

In the new /private/tmp/suss-stack-69 worktree, `gh stack checkout 69` imported
11 active branches (PRs #49 and #59–#68). Existing occupied stack worktrees were
detached at their original commits, preserving their files and the primary
worktree's untracked reference material; the original mapping is recorded in
/private/tmp/suss-stack-69-original-worktrees.json.

`gh stack rebase` fetched main 86e3852 and stopped on one handoff append conflict.
Retained both the proposed ADR-0001 publication evidence from main and the
immutable module implementation evidence. `GIT_EDITOR=true gh stack rebase
--continue` then successfully rebased all 11 branches. Before this evidence entry,
the top tree differed from its previous remote head only by the six documentation
files introduced by main; no implementation, test, inventory or dependency changed.

`git diff --check origin/resurrection/portable-dynamic-bindings HEAD` passed.
`python3 -m unittest discover -s scripts -p 'test_*.py'` passed all 54 tests,
zero skips, exit 0; /private/tmp/suss-stack-69-python.log. No local Rust baseline
was repeated for this documentation-only rebase; existing results apply to the
unchanged implementation. Independent per-PR rebase-integrity review is pending,
followed by `gh stack push` and CI on every exact final head. This entry does not
claim those pending results. No PR was merged and no acceptance status changed.
Next unblocked task: finish rebase review/push/final-head CI, then continue the
existing production frontend/core prerequisites and open roadmap work.

## Next ExceptionInfo work started — 2026-09-30

Isolated worktree /private/tmp/suss-portable-exception-info, branch
resurrection/portable-exception-info, fast-forwarded onto PR68 independent review
7aa3c532dbbb1613ce2d0bae6173c09af0669286. Reviewer reported no significant
production defect; added two semantic regressions, native58/full workspace and
fresh278/16 pipeline/Python54 pass, all handles terminal. Root ff-synced PR68
worktree. Exact reviewed-head CI36715066879 was queued; no readiness claim yet.
Predecessor CI36714342812 was still running and cannot establish final readiness.

New original native portable_exception_info target contains five tests for core
constructors/getters, arbitrary raw message/data/cause values, nominal class
identity versus same-shaped user objects, typed catches, retained closures through
frame exit/GC, first-class canonical live bindings, lexical shadowing and arity
argument order/recovery. Actual focused command with shared target/two build jobs:
cargo test -p suss-cli --test portable_exception_info --locked -- --test-threads=2
failed all5, zero ignored, at unresolved ex-info/ExceptionInfo/getter names;
/private/tmp/suss-exception-info-native-red.log, handle38115 terminal101.
These are failing regressions for missing support, not a successful implementation.

Read pinned core.cljs11756–11823 and exact inventory hashes: constructor retains
supplied raw values; ex-info has fixed2/3 arities; getters distinguish ExceptionInfo
and Error identity. No copied upstream source. Original18-case pinned reference
probe is compiling in initialized arithmetic-values development oracle worktree,
/private/tmp/suss-exception-info-reference.log, inputs
/private/tmp/suss-exception-info-reference-inputs.json. Includes raw falsy values,
first-class getters/redefinition, ordinary/same-shaped values, nominal catch,
Error-parent catch and missing constructor arguments. Exact Node observations must
be obtained and validated before source corpus additions; no result claimed yet.
Next implement full portable constructor/getter behavior with shared rooted identity,
source core cells and phase/alias handling, execute focused/fresh/full gates, then
publish a stacked PR with independent review and exact-head CI. Persistent map data,
printing/error stack adaptation and full ExceptionInfo/core import acceptance must
remain explicit until their own executing evidence exists. No new PR or issue closure.

Primary ExceptionInfo probe compile and Node execution are now both terminal exit0,
/private/tmp/suss-exception-info-reference.log and -reference-observations.json.
Strict validation checked all18 ordered IDs, exact schema and expected binary64
bits against recorded inputs. Raw false message/data/cause survive distinctly;
missing constructor fields are not identical to nil, while ex-info's omitted cause
is nil. The pinned exception also matches js/Error catch; JS source interop itself
remains outside the portable source promise, so any portable Error alternative needs
explicit adaptation documentation. No copied core form or implementation claim.
Added sixth native regression for exact false/nil/undefined storage, not yet run;
first5 actual red regressions remain preserved. Next execute expanded red baseline
then implement; PR68 reviewed-head CI remains running, not yet successful.

Expanded ExceptionInfo native red target terminal exit101, handle1574:
/private/tmp/suss-exception-info-native-expanded-red.log. All6 fail at missing
core bindings, zero ignored; Rust fixture compiles. All root native/reference
processes terminal. These tests and handoff remain uncommitted WIP on the isolated
ExceptionInfo branch; no implementation, PR or readiness claim yet.

ExceptionInfo first implementation uses original descriptor-backed UserObject7,
existing universal closures and canonical bootstrap core cells. All6 original native
regressions passed, -native-first.log. Subsequent inspection found getters and
construction must consult the live ExceptionInfo class cell rather than freezing
its descriptor. Core closures now capture that cell and use shared binding-get,
constructor-descriptor/source-constructor and Invoke. Initial live factory change
failed all6 because a scoped raw Val was reused after its RootScope ended,
-native-live.log. Fixed by retaining/re-reading the rooted native Global; all7
(including live class redefinition) pass, -native-live-fixed.log. Logs prefix
/private/tmp/suss-exception-info; source/provider/helper changes remain WIP.

Expanded primary20 probe compiled and executed exit0, -reference-expanded.log and
-reference-expanded-observations.json; strict ID/schema/binary64 validation passed.
Ordinary constructor call does not throw under this pin, extra fields are ignored;
raw host-global return semantics are not yet modeled/certified. No such behavior is
silently classified supported. Source-based property getters must preserve names,
not positional assumptions when the class binding changes.

Added eighth regression for reordered/missing fields under a redefined class.
Actual focused run -named-fields-red.log terminal101:7 pass/1 fails when positional
ex-data returns a String instead of numeric field. This is a real semantic bug to
repair before readiness. Broader CLI focus -native-expanded.log stopped on existing
persistent reset count assertion4 versus actual9 core cells;32/33 passed. Updated
that resident bootstrap assertion to9 (four arithmetic plus five ExceptionInfo),
not yet rerun. Dynamic/exception suites were not reached by that failed graph and
are not claimed passed on this candidate. Full/new fresh source/review/CI not run.
Next implement checked field-name lookup/missing-field undefined semantics, source
host provider initialization, expand reference/fresh corpus and rerun focused/full.

PR68 reviewed7aa3c532dbbb1613ce2d0bae6173c09af0669286 CI36715066879 completed
success; root inspected actual full log /private/tmp/suss-pr68-final-ci.log, updated
PR body/readiness evidence and comments5911611653 (#11)/5911612221 (#9).
Authoritative follow-up discovered external rebasing/merge: PR68 actual head is
6b5c5eddfcd42d50bfb3fb67f265ff5a4f182f83, merged2026-09-30T12:48:41Z into
614ae356d6d4afd2e1ff2337a65d17cfd8874914; no open PRs remain. Root issued no
merge command. Rebased-head CI36717209838 was running and cannot be called green
from predecessor evidence. origin/main fetched; compared to7aa only documentation
adds493-ish lines (diffstat492), source trees identical. Next safely rebase this
isolated uncommitted ExceptionInfo WIP onto actual main, preserve new upstream
handoff/ADR text, read dated design changes; do not adopt proposed ADR policy as
accepted. No new PR/issue closure or goal-completion claim. All local native/reference
handles in this turn terminal.

ExceptionInfo WIP is now rebased onto actual merged main614ae356; source changes
were preserved via a local stash. Handoff append conflict resolved by retaining
all new main evidence and the full ExceptionInfo append. Owned stash remains as
a backup until this WIP is committed; no unrelated work was changed.

## ExceptionInfo named properties and executing source expansion — 2026-09-30

Current isolated branch based on actual merged main614ae356; accepted design,
roadmap/inventory and latest handoff read. Proposed ADR remains unaccepted.
Named-field lookup now checks descriptor schema/field storage and UTF-16 names,
uses property names after class redefinition and returns undefined for absent
fields. Info descriptor stores actual message/data/cause names. Native entire
focus terminal0, -native-named-fixed.log:private5/persistent33/dynamic10/exception10/
Info8 =66 pass. Field-order regression first failed7pass/1fail, -named-fields-red.log.
All source operands remain evaluated once before universal runtime invocation.

Standalone actual source host now initializes canonical core class/getter cells and
passes the same rooted class cell to factories. First compiler source expansion
failed on unsupported identical? used only as a test predicate, -compiler.log.
Replaced those predicate tests with stronger direct decoded false values and
undefined arithmetic observations; primitive identity support itself remains
unimplemented and is not claimed successful. Original exploratory cases retained.
Source inputs prior278 unchanged. Fresh current300 comparison/16 actual pipeline
tests passed, -oracle.log, terminal0. Added22 Info source observations, including
raw false/nil/undefined, named fields, live class, constructor arities and ordinary
values. Native storage tests also independently distinguish exact sentinel values.

Compiler focus -compiler-direct-values.log passed source suites, then failed a
nominal test's hardcoded first identity8 vs9 after reserving Info. Repaired the
forged-descriptor fixture to take the actual allocated numeric ID and assert the
collision, preserving the actual reference-identity test. First repaired ABI run
failed because three malformed-fixture callers still passed two instead of three
arguments, -runtime-abi-fixed.log; fixed those calls. ABI13 then passed,
-runtime-abi-fixed2.log. Added independent malformed named-schema invocation
regression; actual ABI14 passed, -runtime-abi-guard.log, typed language exception
rather than trap after GC. No failure skipped or classified green.

Review overlay16 in-progress/1049 unassessed and inventory1065 pass. Python54 pass,
-python.log. Original implementation retains upstream provenance and EPL in the
pinned submodule; no core form copied, dependency/lock/helper bytes unchanged.
Descriptor and per-runtime realm roots add globals but preserve ABI1/prelude and
compilerformat2. Reserved numeric identity counter now starts above those roots;
actual descriptor identity is reference identity, not the counter value.

Further fidelity check: ordinary pinned ExceptionInfo calls explicitly return their
realm object, so the initial undefined callback was observably falsey. Original
bootstrap now retains a truthy per-runtime realm object with non-Info descriptor
and mutable message/data/cause fields; missing fields use undefined. Ordinary
source calls and GC/nonmembership regression pass:Info9 plus ABI14, -ordinary.log,
terminal0. This is a portable realm adaptation, not JS global-property interop;
ordinary-object coercion/prototype surfaces, printing/stack, persistent map data,
full Error classes and compiled core/macro import remain unfinished.

Added3 ordinary-call source cases, now303 total. Fresh expanded oracle is running
alone, /private/tmp/suss-exception-info-oracle-expanded.log. No outcome yet claimed.
Next verify it, run required full locked workspace baseline with shared target/two
jobs/test-threads2, then commit/push draft PR against actual main, dispatch independent
review, push significant fixes and require exact final-head CI. Current changes remain
uncommitted; no Info PR exists. Issues9/11 and M2–M9 remain incomplete.

Expanded fresh oracle terminal exit0, handle22998:303 source observations match,
16 actual pipeline tests pass, -oracle-expanded.log. Docs/corpus evidence updated;
original278 sources unchanged. All local native/reference handles terminal.
Required full cargo test --workspace --locked -- --test-threads=2 now starts alone,
/private/tmp/suss-exception-info-workspace.log, shared target/two build jobs.
Verify terminal outcome before publishing readiness; no PR/review exists yet.

ExceptionInfo full baseline still confirmed live, handle59201, now through most
legacy expressions. Source/code frozen while it runs; no parallel native graph.
Publishing a draft candidate permits independent diff review while final local
baseline runs. Reviewer must wait for root's release before editing or testing.
Readiness still requires terminal full baseline, significant fixes and exact CI.

ExceptionInfo candidate full required baseline terminal exit0, handle59201:
CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2
cargo test --workspace --locked -- --test-threads=2,
/private/tmp/suss-exception-info-workspace.log. Includes native67, selected portable
compiler83 with ABI14/source303, legacy317/12 existing ignores, components29,
conformance9/2 manual ignores, oracle4/1 manual capture ignored, reader/core and
all doc targets including suss_cli. Existing ignores and legacy9pass/7fail
baseline unchanged. All local native/reference handles terminal.

Committed26fd461bc1bf94da2ba14a9eb5a83a332caee0df and pushed branch
resurrection/portable-exception-info. Draft PR #70 against main uses Refs #9/#11;
independent review must push significant fixes and final-head CI must pass before
readiness. No merge or issue closure. Root owns no native graph now; reviewer may
run bounded focus/full/fresh checks without overlap. Next resolve review findings,
verify exact CI, then continue complete Error/core/collection/compiler requirements.

## Independent PR70 ExceptionInfo review — 2026-09-30

Reviewed candidate bc69bd6543d605bc11755f8191267205eaf59c9e against main
614ae356d6d4afd2e1ff2337a65d17cfd8874914 in isolated
/private/tmp/suss-review-pr70. Read accepted design/roadmap/inventory/handoff and
pinned core.cljs11756–11823; proposed ADR-0001 remains unaccepted.

Significant finding: the original selected two-argument ex-info implementation
must call the live ex-info binding with nil cause. The candidate directly constructed
an object and ignored that body-level self-call. Runtime closures now retain the
canonical class and self binding cells; native and standalone hosts initialize the
self cell before publishing its factory value. Three-argument captured implementation
still constructs using the live class. No shared prelude/ABI version/compiler format
or dependency/helper bytes change.

Primary pinned probes compiled and executed successfully: fixed replacement input
/private/tmp/suss-review70-reference-fixed-input.cljs, -reference.log and
-reference-observations.json; multi-arity replacement input retained in isolated
ignored tests/oracle/out/generated/suss_oracle/review70.cljs, -reference-multi.log
and -reference-multi-observations.json (all logs share /private/tmp/suss-review70).
The multi-arity replacement yields generic saved/local2=41 and3=42, but apply saved2=42
and3=original data7. Fixed fn3 replacement produces missing JS arity-slot TypeErrors
for generic saved/local calls and apply2; apply3 retains original data7. These raw
primary differences are not called source successes. Current universal invocation
selects a captured arity implementation, matching the apply selected-arity observation.
Generic JavaScript wrapper/global-slot dispatch remains explicitly unresolved.

Initial native regression failed, -live-overload-red.log. First repair focus passed10,
-focused.log, compiler pipeline16/ABI14 passed, -compiler.log. Expanded final native
focus first failed11/1 because its fixture def shadowed the canonical binding in user
namespace; repaired by explicitly defining in suss.core, -focused-final2.log:
12 pass, zero ignored, exit0. Independent additions cover selected arity invocation
through GC/redefinition, ordered once-only source arguments and ex-message's original
Error family after class replacement. This does not certify incomplete Error source
surfaces, printing/stack, source field mutation, persistent map data or compiled core.

Python54 and overlay16 reviewed/1049 unassessed passed, -python.log/-reviews.log.
Full required workspace baseline passed exit0, -workspace.log, handle55383 terminal,
before the last test-only additions. Fresh303/16 pipeline oracle is currently running
alone, handle12979, -oracle.log. Final full baseline must follow that graph and verify
all12 Info regressions. No merge or issue closure; exact final pushed-head CI remains
required before readiness. Root owns no native graph while this reviewer is testing.

Independent fresh oracle terminal exit0, handle12979:303 pinned source observations
match and16 actual decoded pipeline tests pass, -oracle.log. Independent final
required cargo test --workspace --locked -- --test-threads=2 terminal exit0,
handle15292, -workspace-final.log, using shared target/two build jobs and no
RUSTFLAGS override. Includes native70 (private5/persistent33/dynamic10/exception10/
Info12), ABI14, selected portable compiler83, source303 and all legacy/component/
reader/core/doc targets including suss_cli. Existing manual/legacy ignores remain
explicit and unchanged. No native/reference process remains live; all review handles
5570/92190/1291/96710/72327/8133/55383/11532/90756/12979/15292 terminal.
Push significant fix and evidence, then require exact final-head CI before readiness.
Next continue complete Error/core/callable/collection/compiler requirements; issues9/11
and M2–M9 remain incomplete. No merge or issue closure.

## Reviewed PR70 readiness and reproducible core import — 2026-09-30

Previous implementation turn made progress: independent PR70 review pushed the
live ex-info self-call fix a3fbc4f131ea351977f6991cf033473fcd1c57de, full local
baseline/fresh303 comparison passed, and exact final-head CI36723153710 passed.
Actual enabled-suite CI log inspected at /private/tmp/suss-pr70-final-ci.log.
PR70 was marked ready, remains OPEN, no merge. Issue9/11 progress comments
5912907944/5912907274 retain Refs and incomplete acceptance. Proposed ADR remains
unaccepted. Current full ROADMAP goal remains active; M2–M9 are incomplete.

Next bounded prerequisite for M4-01/issue16: reproducible reviewed upstream form
extraction/explicit adaptation and license packaging. Isolated worktree
/private/tmp/suss-core-import, branch resurrection/reproducible-core-import, is
based on exact reviewed PR70 head a3fbc4f. Root repository's unrelated untracked
reference/BUSINESS_DSL_RESEARCH.md and reference/clojure-site/ are preserved.

New scripts/core_import.py selects reviewed IDs from the exact pinned source/form
inventory, records UTF-8 byte ranges/context and source hashes, verifies clean
source and byte-preserved pinned license files, checks explicit source-hash-bound
single-declaration patches, and reproduces runtime/core-import. Manifest records
source/tool/recipe/review/inventory/patch/artifact/license hashes plus dependencies
and semantic test references. Original extracted form/notices are retained. Extra
or changed/missing generated files fail checks rather than being silently removed.
Duplicate IDs/JSON keys, stale/unreviewed/excluded forms, phase mismatch, missing
notices and escaping input paths fail. This is provenance, not semantic certification.

First selected form runtime:identity:2691 retains upstream source/EPL-1.0 notice
and full byte-preserved LICENSE/epl-v10.html. Explicit patch converts the single
fixed defn to def/fn for the bounded bootstrap compiler, retaining the original
body; original docstring remains in extracted source. No full compiled defn macro,
metadata API or automatic production core loading is claimed. Review overlay now
17 in-progress /1048 unassessed, no new exclusion or completion. Canonical suss.core
artifact executes through the persistent Session; cljs.core aliases the same cell.

Commands/results (two native jobs/shared target, no RUSTFLAGS override; all native
Cargo graphs sequential):

- New tool tests first failed missing core_import module, /private/tmp/suss-core-import-red.log; implementation now has11 focused positive/negative provenance tests. No semantic success inferred from this tooling failure.
- Initial executing native import focus:3 passed, zero ignored, exit0, /private/tmp/suss-core-import-native.log (65903 terminal). Tests prove absence before load, exact scalar/object/function identity through GC, canonical aliases/live redefinition/old closures, once-only argument evaluation and typed arity-error recovery.
- Initial primary runner failed unknown invented utf16 transport tag, /private/tmp/suss-core-import-oracle.log (55497 terminal1); corrected to existing strict string tag. Next fresh14 observations passed but native corpus fixture failed to compile because CLI lacked serde_json dev dependency, /private/tmp/suss-core-import-oracle-fixed.log (6708 terminal101). Added dev-only existing serde_json1; offline lock update changes only suss-cli dependency edge, no crate version/new package/runtime dependency.
- Expanded native focus:4 passed, zero ignored, /private/tmp/suss-core-import-native-corpus.log (56768 terminal0). Shared14-case scalar corpus is independently decoded through actual imported Suss execution; malformed/wrong layouts and unknown expected tags fail. Exact sentinels/binary64/UTF-16 retained.
- Final CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2 sh scripts/test-core-import-oracle.sh:14 fresh pinned upstream observations match, actual native4 passed, exit0, /private/tmp/suss-core-import-oracle-final.log (29227 terminal). Primary runner invokes upstream identity, not the patched form; JVM/Node remain development-only.
- Python suite65 passed, exit0, /private/tmp/suss-core-import-python-final.log. Inventory1065/overlay17, WIT15/six packages, numeric integrity and offline roadmap preview pass. Generated artifacts reproduce byte-identically; shell syntax/touched Rust formatting/diff checks pass. CI now verifies import provenance alongside existing inventory and full workspace checks.
- Required CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2 cargo test --workspace --locked -- --test-threads=2 passed exit0, /private/tmp/suss-core-import-workspace.log (95575 terminal). Includes native74 (four new core-import tests), ABI14/source303 and all legacy/component/reader/core/doc targets including CLI. Existing manual/legacy ignores and legacy9pass/7fail remain explicit and unchanged.
- Bounded scripts/verify-core-import.sh passes exact inventory/overlay/artifact checks and native4, /private/tmp/suss-core-import-verify.log (54569 terminal0). It is prerequisite evidence, not a full M4/M7 gate.

Publish stacked on PR70 with Refs #16, dispatch independent PR review, push
significant findings and require CI on exact final reviewed commit. Do not merge.
M4-01 stays in progress: full reviewed dependencies, macro/phase bootstrap,
namespace privacy/doc metadata, collection foundations and automatic production
core loading remain incomplete. Next extend the reviewed core foundations needed
for collection ports while implementing the compiled macro/core bootstrap; do not
replace upstream behavior with opaque encoding-only collection stubs or claim all
portable forms load. Keep stable roadmap IDs and source provenance aligned.

## Independent PR71 core-import review — 2026-09-30

Reviewed exact candidate 0b87bf9b948d9b661bd4835fbac4fffff8d9d4e2 against
reviewed PR70 a3fbc4f131ea351977f6991cf033473fcd1c57de in isolated
/private/tmp/suss-review-pr71. Significant finding: extraction retained source
context only as manifest metadata, but accepted contextual selections and emitted
them as unconditional top-level declarations. This could activate a :clj-only
reader branch or discard a let initializer's captured binding while producing a
valid provenance manifest. Independent negative build regressions for both cases
failed before repair (two accepted invalid contexts),
/private/tmp/suss-review-pr71-red.log. Import now rejects any nonempty declaration
context with an explicit unsupported diagnostic before adaptation/output. Exact
source ranges/context remain in the inventory; context-preserving import is future
work. Selected identity has empty context, so its source semantics stay unchanged.
Updated import documentation, corrected review-count arithmetic, and regenerated
the manifest's importer hash. No upstream form, patch, review, license or runtime
ABI changes. No semantic success is inferred from packaging/source hashes.

Python66 passed, /private/tmp/suss-review-pr71-python.log. Focused native core-import4
passed, zero ignored, /private/tmp/suss-review-pr71-focused.log, handle59614 terminal0.
Fresh14 pinned primary observations and all4 actual native imported-form tests passed,
/private/tmp/suss-review-pr71-oracle.log, handle99199 terminal0. Inventory1065,
overlay17 in-progress/1048 unassessed and regenerated artifact verification pass.
Required CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target
CARGO_BUILD_JOBS=2 cargo test --workspace --locked -- --test-threads=2 passed exit0,
/private/tmp/suss-review-pr71-workspace.log, handle13853 terminal0. All native,
compiler, legacy, component, reader/core and CLI doc targets passed; existing
manual/legacy ignores remain unchanged. No native/reference process remains live;
all review handles59614/99199/13853 terminal and shared native graph released.
No RUSTFLAGS override; shared target and two build jobs. Require exact final pushed-head CI; no merge or issue closure.
M4-01 remains incomplete: context-preserving import, dependency/core/macro/phase
bootstrap, collections and automatic production loading remain next work.

## PR71 readiness and portable primitive predicates — 2026-09-30

Previous goal turn made progress: published draft PR71 and dispatched independent
review. Reviewer found a significant semantic provenance defect: contextual reader
branches/lexical definitions could be flattened into unconditional source. Two
independent regressions failed before repair. Reviewed19cf741df410221201bf39e8a2cc999a7afca18b
now rejects nonempty context explicitly until context-preserving import exists;
retains actual context in inventory and updates docs/manifest. Independent native4,
fresh14 primary observations, Python66 and full workspace passed. Exact final-head
CI36728380154 passed; actual suite log /private/tmp/suss-pr71-final-ci.log inspected.
PR71 marked ready, remains OPEN/unmerged; issue16 progress comment5913673258 keeps
Refs. No agent merge or issue closure. All reviewer/native/reference processes terminal.

Next core/collection prerequisite: seven primitive predicate bindings and strict
identity on the shared runtime. Isolated /private/tmp/suss-scalar-predicates, branch
resurrection/portable-scalar-predicates, based on reviewed19cf741. Canonical runtime
cells provide nil?/false?/true?/undefined?/number?/string?/identical? with fixed
universal arities. Runtime nil? treats internal missing-field undefined as nil-like,
while undefined? and identical? distinguish it. Identical? compares Number f64
values (signed zeros match, NaN always differs), UTF-16 code units, and other
language objects/functions by reference. No operand/source expression is re-emitted.

Original Rust/Wasm callbacks use existing INVOKE closure type and append factory
exports without changing the stable ten-type prelude, ABI1/compilerformat2, numeric
helper bytes or dependency versions. Native Session initialization automatically
installs no-environment factories; standalone test hosts initialize the same names.
Bootstrap resident cells are now16, up from9; reset regression preserves this count.
Runtime/Macro phase core names remain separate compiler cells, but compiled predicate
macro/metadata/core import is explicitly unfinished, not certified by runtime tests.

Seven runtime inventory entries retain exact pin/hashes/arities and primary macro
dependencies;24 in-progress/1041 unassessed. No item newly marked implemented/excluded.
These intrinsics copy no upstream forms; the existing identity source import retains
its EPL source/notice/license files. Its manifest is regenerated for the changed
review overlay hash, not new extracted forms. Only one upstream form is still imported.

Commands/results (shared target/two native jobs; no RUSTFLAGS override; native/oracle
Cargo graphs sequential, original repository's unrelated untracked files preserved):

- New four executing native regressions failed unresolved nil?/identical? before implementation, /private/tmp/suss-scalar-predicates-red.log, handle58753 terminal101. Primary source reviewed at pinned core.cljs241–258/307/2323–2339 and core.cljc923–937/988–998/1017–1035.
- First focus: native4 pass, zero ignored, /private/tmp/suss-scalar-predicates-focused.log (84993 terminal0). Covers exact primitive distinctions including NaN self-identity and UTF-16 concatenation, nominal/functions after GC, aliases/live cells/original captures, callee/argument order and language arity recovery.
- Surrounding native private/session/core-import/dynamic/exception/Info/predicate focus:78 pass, zero ignored, /private/tmp/suss-scalar-predicates-native.log (81649 terminal0). Reset's new cell count is exact rather than hidden.
- Expanded corpus retains prior303 source cases unchanged and adds94. Fresh397 primary observations initially passed, but runtime suite failed only its obsolete final303 count assertion after comparing every case, /private/tmp/suss-scalar-predicates-oracle.log (30606 terminal101). Updated count397; no value/failure/skip expectations changed.
- Compiler pipeline16 and runtime ABI14 pass, /private/tmp/suss-scalar-predicates-compiler.log (21113 terminal0). Full ABI validation/execution and earlier schema/identity regressions preserved.
- Final CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2 sh scripts/test-portable-pipeline-oracle.sh:397 fresh pinned observations match,16 actual independently decoded pipeline tests pass, /private/tmp/suss-scalar-predicates-oracle-final.log (4963 terminal0). No skip/new failure accepted. Separate core-import14 and legacy9pass/7fail remain separate evidence.
- Python66 pass, /private/tmp/suss-scalar-predicates-python.log. Inventory1065/overlay24+1041, core artifact reproduction, WIT15/six packages, numeric integrity, offline roadmap preview and diff checks pass.
- Required CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2 cargo test --workspace --locked -- --test-threads=2 is currently running alone as handle44781, /private/tmp/suss-scalar-predicates-workspace.log. Final result must be recorded before publication/readiness.

Next finish the baseline, publish stacked on PR71 with Refs #9/#16, dispatch
independent review, push significant findings and require exact final-head CI.
Do not merge. After these intrinsics, extend provenance-backed not/boolean/some?
source foundations needed by collection ports and compiled macro/core bootstrap;
then continue sequence/collection and full M2/M3 requirements. Do not mistake strict
primitive identity for persistent equality/hash or claim compiled predicate macro
expansion/redefinition semantics complete. M2–M9 and the full ROADMAP goal remain active.

Predicate full baseline terminal exit0, handle44781, same required command/log.
All workspace targets including CLI doc tests passed. Includes the focused shared
session/runtime78, compiler ABI14/source397, legacy317/12 existing ignores,
components29, strict conformance9/2 manual ignores, full oracle4/1 manual capture
ignored and reader/core/documentation targets. Existing ignores and legacy9pass/7fail
unchanged. All root native/reference handles terminal; independent reviewer may
own the next exclusive graph. Publish and review remain required; no merge/closure.

## Independent PR73 scalar-predicate review — 2026-09-30

Reviewed exact candidate e2c87cc10f19835fb4624298bbb345ce351eba01 against
reviewed PR71 19cf741df410221201bf39e8a2cc999a7afca18b in isolated
/private/tmp/suss-review-pr73. Inspected pinned runtime/macro source, sentinel
Booleans and nil/internal-undefined distinction, Number NaN/signed-zero/infinity
identity, UTF-16 unit identity, nominal/function reference identity, factory
registration and central arity, canonical live cells/aliases and phase-separated
compiler availability, native/standalone hosts and independently decoded corpus.
No significant semantic defect found within the explicitly bounded runtime scope.
Compiled upstream predicate macros and their redefinition behavior remain
unfinished; this review does not certify them or persistent equality/hash.

Added two independent executing regressions: all six unary factories remain
first-class/rooted through collection and reject both wrong arity boundaries;
a long astral/lone-surrogate string identity comparison exhausts operation fuel,
then its retained function/inputs survive collection and succeed with a larger
budget. Length mismatch and the next ordinary predicate input also recover.
No production semantic change, copied upstream source, dependency/helper/prelude
or ABI/compiler-version change. Native predicate suite now has six regressions.

Externally merged PR70/71 changed main to c713c80f349f5385c439d800aafc5b3e63cdea96.
Its tree exactly matched reviewed19cf741. Rebased only PR73 and its review tests
onto that main; complete pre/post-rebase source-tree diff was empty. No agent merge.

Commands/results (exclusive native graph, shared target, two jobs, no RUSTFLAGS):
- Candidate native4 passed, /private/tmp/suss-review-pr73-focused.log, handle26379 terminal0.
- Expanded native6 passed with zero ignored, /private/tmp/suss-review-pr73-expanded.log, handle45905 terminal0.
- Fresh397 pinned primary observations matched exactly; actual portable pipeline16 passed, /private/tmp/suss-review-pr73-oracle.log, handle7185 terminal0. Corpus remains397; no expectation, failure or skip weakened.
- Python66, inventory1065, overlay24 in-progress/1041 unassessed and five reproducible core-import files verified, /private/tmp/suss-review-pr73-python.log, handle32110 terminal0.
- Required CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2 cargo test --workspace --locked -- --test-threads=2 passed exit0, /private/tmp/suss-review-pr73-workspace.log, handle18963 terminal0. Includes native predicate6, surrounding native/session tests, actual pipeline397/16 and ABI14, legacy/component/conformance/oracle/core/reader and CLI doc targets. Existing legacy/manual ignores and legacy9pass/7fail evidence unchanged.

All review native/reference/process handles terminal and native graph released.
Push rebased review head to PR73, retarget it to main, and require exact final-head
CI before readiness. Refs #9/#16 remain partial; no issue closed or PR merged.
Next unblocked task: provenance-backed not/boolean/some? foundations followed by
sequence/collection ports and complete compiled macro/core bootstrap. M2–M9 and
the full roadmap remain incomplete.

## PR73 final review and source-backed boolean ports — 2026-09-30

Previous goal turn made progress: primitive predicates/identity implemented, tested
and published as draft PR73 with dispatched independent review. The reviewer found
no significant semantic defect; added all-factory first-class/GC/arity and long
UTF-16 identity fuel-recovery regressions. Final native6/fresh397/pipeline16/Python66
and required full baseline passed. Externally merged PR70/71 main trees match the
reviewed base; reviewer rebased only predicate work onto actual mainc713c80 with
source tree preserved, then pushed c4a36760028b5cbec2abc3d53597b9a173346f62.
All review handles terminal, including18963 baseline and30296 push; no agent merge.

PR73 retargeted to main. No CI check existed on the rebased final head: PR/check-run
and workflow-run APIs showed only successful predecessor e2c87cc run36732518761.
Closed/reopened draft PR73 to trigger its configured reopened event without a code
or head change. Exact c4a3676 run36734926071 is now confirmed IN_PROGRESS. It is not
called successful and PR73 remains draft until that exact run passes. Root's local
old predicate worktree still preserves e2c87cc; next branch starts from reviewed c4.

Next source-backed core prerequisite in isolated /private/tmp/suss-boolean-core-import,
branch resurrection/portable-boolean-core-import, based on c4a3676: not and boolean
join identity in the reviewed extraction recipe. Original source forms/docstrings,
exact byte ranges/hashes and EPL notice/files remain retained. Explicit whole-form
patches adapt fixed defn/cond/nil?/false? macro logic to existing def/fn/if bootstrap
forms. If preserves falsey nil/false/internal undefined and truthy numeric zero,
NaN, empty UTF-16 strings and objects; it avoids a wrong live nil?/false? dependency
where the pinned functions use compiled primitive macros. Captured not/boolean
originals survive public predicate redefinition; current core var reads still see
not/boolean replacements. This behavior is executed against the actual primary oracle.

No compiler/runtime implementation/dependency/prelude/ABI/helper changes. Generated
artifact now contains three source forms and seven files, with the existing exact
license bytes. Two new partial runtime reviews retain actual macro:defn:3364,
macro:cond:159, macro:nil?:923 and macro:false?:991 dependencies. Overlay26 in-progress/
1039 unassessed; neither form newly certified complete. Native command frontends,
compiled upstream macros, source metadata/privacy, full core/collection loading and
some? remain unfinished. Some?'s nil? macro versus live not binding needs an explicit
primitive/compiled macro adaptation; do not substitute a default-only equivalent.

Commands/results (two native jobs/shared target, no RUSTFLAGS override; graphs
sequential; unrelated root untracked files preserved):

- Three added source-import regressions first failed unresolved not/boolean, /private/tmp/suss-boolean-core-import-red.log (95960 terminal101). Source pin is unchanged c4295f303100bbf5afac449242d30bca1126f1a1; core.cljs263–269/2357–2364 and referenced core.cljc macro declarations reviewed. An initially guessed cond dependency ID was corrected to actual159 before final provenance generation; no incorrect ID is claimed as validated semantics.
- Expanded native core_import7 passed, zero ignored, /private/tmp/suss-boolean-core-import-focused.log (69405 terminal0). Includes truthiness over primitives/objects/missing fields/ordinary realm value, canonical aliases, captured pure functions versus redefined predicate vars, live not/boolean replacements, once-only argument effects before typed arity errors and recovery; existing identity regressions retained.
- Fresh CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2 sh scripts/test-core-import-oracle.sh passed:50 independently encoded primary observations and actual native7, /private/tmp/suss-boolean-core-import-oracle.log (12758 terminal0). Previous14 cases unchanged;36 new truthiness/first-class/alias/redefinition cases. Development-only JVM/Node remain outside the shipped runtime. Primary arity warnings for ordinary ExceptionInfo calls remain raw, not hidden as skips.
- Python66 passed, /private/tmp/suss-boolean-core-import-python.log. Inventory1065/overlay26+1039, exact seven-file artifact regeneration, offline roadmap preview, touched Rust formatting and diff checks pass. Stable issue IDs and partial status preserved.
- Required CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2 cargo test --workspace --locked -- --test-threads=2 passed exit0, /private/tmp/suss-boolean-core-import-workspace.log (10064 terminal). All workspace/native/component/legacy/reader/core/doc targets including CLI pass; source pipeline397 and existing manual/legacy ignores remain unchanged. Existing legacy differential9pass/7fail remains separate.

Next publish stacked on PR73 with Refs #16, dispatch independent review, push
significant findings and require exact final-head CI. Do not merge or close issue16.
Continue complete core/bootstrap/collections and M2/M3/M4 prerequisites; three forms
and two Boolean operations are not the full M4 gate. The full ROADMAP goal remains
active, M2–M9 unfinished. Root owns no native/reference process after this baseline;
a dispatched reviewer may take exclusive graph ownership.

## Independent PR74 source-backed Boolean review — 2026-09-30

Reviewed candidate446f2276bc6e4f8e717f1107c51484a8ca8d878d against independently
reviewed PR73 c4a36760028b5cbec2abc3d53597b9a173346f62 in isolated
/private/tmp/suss-review-pr74. No significant defect found in the bounded runtime
adaptations. Checked exact pinned source ranges/hashes, macro dependencies,
empty declaration context, retained originals/docstrings/notices, whole-form
patches, three-form dependency order, deterministic manifest and packaged EPL
files. Actual license bytes match upstream with cmp. Pinned nil? expands loose
nil equality (including internal undefined), false? strict false; portable if
preserves their union without reading mutable public predicate vars. Source-backed
not/boolean still observe canonical live var replacements and retained original
functions. Compiled upstream macros, privacy/doc metadata, production automatic
core loading, some? and collections remain incomplete; no full core claim.

Added an independent executing regression for namespace aliases, callee capture
before argument set! replaces that cell, core reload, retained originals and a
retained replacement across GC. An initial fixture used an unsupported def into
another namespace and correctly received an explicit diagnostic; repaired the
fixture to supported global set!. This was not a production semantic failure or
fix. Corpus remains50 (prior14 retained plus36 Boolean cases); overlay remains26
in-progress/1039 unassessed and imported forms remain3. No runtime/compiler,
source patch, manifest, dependency, helper/prelude or ABI change.

Commands/results (exclusive graph, shared target/two jobs; no RUSTFLAGS override):
- Candidate native7 passed, /private/tmp/suss-review-pr74-focused.log,94223 terminal0.
- Expanded first fixture7 pass/1 rejected cross-namespace def, /private/tmp/suss-review-pr74-expanded.log,62130 terminal101; repaired native8 passed, /private/tmp/suss-review-pr74-expanded-final.log,74457 terminal0. No test weakened or skipped.
- Fresh50 pinned primary observations match exactly, native8 pass with zero ignored, /private/tmp/suss-review-pr74-oracle.log,78877 terminal0. Raw primary wrong-arity warnings for ordinary ExceptionInfo calls remain recorded.
- Python66 passed, /private/tmp/suss-review-pr74-python.log; inventory1065, overlay26+1039 and deterministic seven-file artifact verified,66619 terminal0. Rust formatting and diff checks passed; LICENSE/epl-v10.html cmp passed.
- Required CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2 cargo test --workspace --locked -- --test-threads=2 passed exit0, /private/tmp/suss-review-pr74-workspace.log,41491 terminal0. All workspace targets including native8, actual source397/pipeline16/ABI14, legacy, component, conformance, oracle, reader/core and CLI doc tests passed. Existing legacy/manual ignores and separate legacy9pass/7fail evidence unchanged.

Every review native/reference/process handle is terminal; shared graph released.
Remote PR73/74 still open on the reviewed stack at publication check. Push only
this regression/review evidence to PR74 and require exact final-head CI before
readiness. Refs #16 remains partial; no issue closure or PR merge. Next unblocked
work: explicit primitive adaptation for some?, compiled core/macro bootstrap and
sequence/collection foundations. M2–M9 and the full ROADMAP goal remain incomplete.

## PR74 readiness and source-backed some? increment — 2026-09-30

Previous goal turn verified merged PR40/41 had no closing issue links; partial
acceptance remained open. Future complete acceptance uses Closes, partial uses
Refs. The preceding implementation turn published the Boolean port; independent
review then pushed d2cd29c7ed7cfe3764c2a943168f1fc5ea6eb2b4. Review native8,
fresh50, Python66, provenance/license and required full baseline passed; all
review handles terminal and graph ownership explicitly released. Exact final-head
CI36739168634 passed; actual enabled-suite log inspected at
/private/tmp/suss-pr74-final-ci.log. PR74 body updated and marked ready, without
merge or issue closure. PR73 remains the reviewed stack base.

Next branch /private/tmp/suss-some-core-import,
resurrection/portable-some-core-import, starts from reviewed d2cd29c. Four source
forms now retain source/patch/EPL provenance; some? uses a typed private
suss.bootstrap/nil? primitive, normalized to one IR value. Its HIR result type,
IR result/dominance/definition, recurrence operand and namespace reservation are
checked. Emission compares the evaluated value to nil/internal undefined, without
re-emitting source operands. No shared ABI/runtime/helper/dependency change.

A source-reading assumption in earlier handoff/body text was WRONG: some?'s
source (not (nil? x)) does not call live not in pinned compiled core. Fresh
captured-function probes under not redefinition returned unchanged Boolean values,
including identity and non-Boolean replacement functions. Generated core.js:
some_QMARK_(x) returns !(x == null). Pinned compiler.cljc1205–1206/1262–1263
optimizes not on inferred Boolean operands. Public nil? redefinitions also do
not alter the compiled primitive. Corrected new corpus/test expectations only
after inspecting these primary observations and exact emitted implementation;
prior50 observations unchanged. PR74 body speculation corrected. This bounded
port does not certify complete Boolean inference/macros/source metadata.

Commands/results, shared target/two jobs/no RUSTFLAGS, sequential native graphs:
- Fresh primary build94324 terminal0, /private/tmp/suss-some-core-import-primary-build.log. Initial 72-case compare failed on the three incorrect live-not assumptions; emitted code and raw observations reviewed, expectations corrected to actual primary behavior. Final strict72 comparisons pass. Raw generated/reference data remain tests/oracle/out.
- Three initial native regressions failed unresolved some?, /private/tmp/suss-some-core-import-red.log,78241 terminal101. Initial focused24083 also failed because unsupported importer --write flag did not regenerate artifact; corrected documented no-argument generation produced eight files. No failure/skip hidden.
- Final native11 passed, /private/tmp/suss-some-core-import-focused-final.log,86790 terminal0. Expanded native12 plus compiler pipeline17 pass, /private/tmp/suss-some-core-import-expanded.log,15771 terminal0. Includes false versus nil/undefined, scalars/objects/GC, ignored predicate/not redefinitions, once-only arguments/arity errors, private operand effects/throws, invalid arities/first-class reads/reserved namespace and non-tail recurrence. Forged HIR/IR result and undefined operand regressions pass.
- Full fresh CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2 sh scripts/test-core-import-oracle.sh passed72 exact primary comparisons/native12, /private/tmp/suss-some-core-import-oracle.log,75764 terminal0. Prior50 retained,22 added; no changed prior expectations/new failures/skips.
- Python66 passed, /private/tmp/suss-some-core-import-python.log,58881 terminal0. Inventory1065/overlay27 in-progress+1038 unassessed and exact eight-file manifest reproduction pass. Rustfmt/diff checks pass; unrelated rustfmt child change restored.
- Required CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2 cargo test --workspace --locked -- --test-threads=2 is live83337 alone, /private/tmp/suss-some-core-import-workspace.log. Must record terminal result before publication/readiness.

Next finish full baseline, publish stacked on reviewed PR74 with Refs #9/#16,
dispatch independent review, push significant fixes and require exact final-head
CI. Do not merge. Then continue source-backed sequence/collection and compiled
macro/core bootstrap prerequisites; public macro inference, metadata/privacy,
complete import/loading/collections and production CLI migration remain unfinished.
M2–M9 and the full ROADMAP goal remain active. No milestone completion inferred.

Some? required full baseline83337 terminal exit0, same command/log. All workspace
targets including CLI doc tests pass; native12 and pipeline17/397 included. Existing
legacy/manual ignores and legacy9pass/7fail baseline unchanged. Root native/fresh
reference handles all terminal; review may take exclusive graph ownership.
Offline roadmap preview, inventory/reviews and exact eight-file regeneration pass.


## Independent PR75 source-backed some? review — 2026-09-30

Reviewed exact candidate555c099e89015aebdc318514e8184d80d2ae70f4 against
reviewed PR74 d2cd29c7ed7cfe3764c2a943168f1fc5ea6eb2b4 in isolated
/private/tmp/suss-review-pr75. No significant production defect found within the
bounded primitive/source-import scope. Inspected private syntax arity/source spans,
reserved namespace/alias APIs, HIR free-variable capture and recurrence traversal,
IR operand definition/dominance and Boolean result typing, and emission using an
already evaluated value with nil/internal undefined sentinels. Both comparisons
read the normalized local; they do not replay source effects. No ABI/runtime,
numeric helper, dependency or upstream source/patch changes.

Verified pinned runtime some? source and compiler Boolean-not optimization;
fresh generated core.js independently confirms some_QMARK_ returns !(x == null).
Captured upstream some? therefore ignores public nil?/not redefinitions, while
its own canonical live cell remains redefinable. Exact retained source, metadata/
docstring, byte range/hash, explicit patch and dependencies, deterministic eight
files and byte-preserved EPL license packaging all checked. Full compiled macros,
source metadata/privacy, complete core loading/collections and production command
migration remain unfinished. Imported forms remain4; overlay27 in-progress/1038
unassessed; separate core corpus72 and portable pipeline397 remain distinct.

Added an executing regression for nested closure capture of primitive nil-test
operands across fragments/GC (nil, false, functions, missing-field undefined), plus
loop-edge replacement and an unselected throwing branch. Added a forged IR
self-use rejection to test nil-test dominance. An initial review-fixture edit
placed this assertion in an older test whose IR contains no NilTest, correctly
failing on the fixture's unwrap; moved it to the intended nil-test test. This was
not a production defect. No expectation weakened, failure hidden or skip added.

Commands/results (exclusive graphs; shared target/two jobs; no RUSTFLAGS override):
- Candidate native12 passed, /private/tmp/suss-review-pr75-focused.log,40069 terminal0.
- Expanded native13 passed, /private/tmp/suss-review-pr75-expanded.log,23426 terminal0.
- Initial compiler fixture16 pass/1 fail, /private/tmp/suss-review-pr75-compiler.log,65827 terminal101; corrected pipeline17 passed, /private/tmp/suss-review-pr75-compiler-final.log,69165 terminal0.
- Python66 passed, /private/tmp/suss-review-pr75-python.log; inventory1065, overlay27/1038, deterministic eight-file artifact and LICENSE/epl-v10.html comparisons passed,83372 terminal0. Touched Rust formatting and diff checks passed.
- Fresh CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2 sh scripts/test-core-import-oracle.sh passed:72 exact independently encoded primary observations and actual native13, /private/tmp/suss-review-pr75-oracle.log,77554 terminal0. Raw primary ExceptionInfo arity warnings remain recorded. Prior50 expectations/cases unchanged; no failures or skips accepted.
- Required CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2 cargo test --workspace --locked -- --test-threads=2 passed exit0, /private/tmp/suss-review-pr75-workspace.log,57109 terminal0. Includes actual native13, source397/pipeline17/ABI14, legacy, component, conformance, oracle, reader/core and all documentation targets including CLI. Existing manual/legacy ignores and separate legacy9pass/7fail evidence unchanged.

All review native/reference/process handles are terminal; shared native and fresh
reference graph ownership released. Push only the two review regressions and
handoff to PR75; require exact final-head CI before readiness. PR73/74/75 remained
open on the unchanged reviewed stack at publication check. No PR merged or issue
closed. Refs #9/#16 remain partial. Next unblocked work: compiled core/macro
bootstrap and sequence/collection foundations; M2–M9 and the full ROADMAP remain
unfinished.

## Native protocol collection prerequisite — 2026-09-30

Previous goal turn made progress: published source-backed some? as draft PR75,
validated full baseline/fresh72 and dispatched independent review. The reviewer
now pushed56038c089d0698cc61d17df1d810d4521d5b0deb; native13/compiler17/fresh72/
Python66/provenance/license and full baseline passed. All reviewer handles terminal,
including57109 baseline and26887 push; graphs explicitly released. Root fast-forwarded
both PR75 and the next worktree to that reviewed head. Exact final-head
CI36743548741 is authoritatively in progress, not called successful; PR75 stays draft.
PR73/74/75 remained open on their reviewed stack at the latest state query. No merge.

Next worktree /private/tmp/suss-native-protocols, branch
resurrection/portable-native-protocols, has UNCOMMITTED preparation for native
protocol dispatch, needed before source-backed nil/string/collection protocols
can load. Production implementation is not changed yet. Do not publish or call
these native tests passing. Added seven semantic native regressions and a separate
22-case development-only strict primary corpus/runner:
- Native nil/undefined/number/boolean/string/function/object dispatch and membership.
- Direct user-object method before native object before wildcard/default fallback.
- Captured method/live extension across GC and multiple arities.
- Native membership reads current replaced protocol value.
- Missing methods preserve ordered argument effects and typed language recovery.
- Captured method fallback consults current method var after replacement.
- Protocol redeclaration resets native fallback tables but direct object methods survive.

Commands/results:
- Initial native test compilation failed only ambiguous Rust float fixture type,
  /private/tmp/suss-native-protocols-red.log; corrected expected10.0f64. No production fix.
- Fresh pinned compiler build62268 terminal0, /private/tmp/suss-native-protocols-primary-build.log; Node terminal0. python3 scripts/native_protocol_oracle.py compare reports22 exact primary observations. Generated core macro output and raw observations remain tests/oracle/out/native-protocol*. No observation skipped or default guessed.
- CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2 cargo test -p suss-cli --test portable_native_protocols --locked -- --test-threads=2 failed all7 as expected on reviewed production code, /private/tmp/suss-native-protocols-red-final.log,54160 terminal101. nil extensions currently raise language errors; number/default names are unresolved. All graphs terminal; no RUSTFLAGS override. This is a failing missing-feature regression, not successful compatibility evidence.

Pinned primary sources core.cljc1332 base-type,1477 base-assign-impls,
2124–2147 expand-dyn and core.cljs322 native-satisfies? explain the result.
Native empty extension returns true (nominal extension uses opaque sentinel).
Captured method functions consult the CURRENT method function's native property
tables: replacing the method var with an ordinary function removes its old fallback;
re-extending the new method makes the old capture call the new implementation.
Redeclaring defprotocol resets new protocol/method native tables but retains direct
user-object slot dispatch. Simply adding primitive descriptors to existing stable
method-key tables would violate these fresh observations.

Next unblocked task: implement closure-owned mutable native property tables and
current-method-cell fallback, preserving direct descriptor dispatch first and
specific/default fallback order. Keep property storage reachable from the owning
closure so replacement can release it with GC; do not use a process-global rooted
registry retaining every replaced closure. The existing ten-type ABI has immutable
Closure fields and a generic environment Value; any environment wrapper must
preserve all callback/environment consumers and actual cross-fragment ABI tests.
Validate storage/lifetime and source effects before classifying support. Preserve
nil/internal undefined normalization, protocol replacement/redeclaration and the
native Boolean extension result. No shipped JS/JVM path, no persistent collection
claim, no issue closure. Array/symbol/bigint native surfaces and complete compiled
macro/core/collection semantics remain broader work; do not silently fabricate them.
Finish exact-head PR75 CI before readiness, then continue full M2/M3/M4 milestones.
The full ROADMAP goal remains active; M2–M9 are incomplete.

## Closure-owned native fallback implementation — 2026-09-30

Previous goal turn made progress: fresh22 primary probes, native7 missing-feature
regressions and issue11 evidence identified current-cell/table lifetime semantics.
PR75 exact reviewed56038c0 CI36743548741 passed; actual enabled suites inspected
at /private/tmp/suss-pr75-final-ci.log. Body updated and PR75 marked ready. User
then externally merged73/74/75:3ed2df3e,9b8517d9,008cb758bbd85ad1f847d4d2094ae77adef71192.
Fetched main; complete tree diff between reviewed56038c0 and main008cb758 is empty.
Stashed only this isolated native work with untracked files, rebased the branch
with no own commits onto that verified main, restored WIP successfully and dropped
only that temporary reconciliation stash. Older unrelated stashes/files preserved.
Next native PR will target main; no agent merge or issue closure.

Original runtime implementation now owns native property tables through a private
UserObject environment wrapper per shipped closure. One tag global appends after
existing globals; no shared prelude/version/numeric helper change. Universal
invocation and class/protocol environment inspectors recover original callback
environments. No globally rooted registry retains replaced functions. Private
GlobalCell IR preserves a method cell reference without a premature var read.
NativeMarker/NativeSet operations and live dispatchers implement direct nominal,
current function's specific kind, then default fallback. Native implementations
replace all arities together and let recur replace their first parameter; source
protocol redeclaration resets native tables while direct descriptor methods persist.
Source arrays/Symbol/bigint/native host properties remain explicitly unfinished.

Commands/results (shared target/two native jobs, no RUSTFLAGS; sequential graphs):
- Initial wrapper checks passed core-import13/runtimeABI14, /private/tmp/suss-native-protocols-properties-focused.log,88054 terminal0. All earlier source-backed forms and ABI callback/root behavior retained.
- First native implementation focus failed generated fragment validation because NativeMarker returned eqref where the emitter's Boolean operation expects i32, /private/tmp/suss-native-protocols-focused.log,34988 terminal101. Corrected private marker result to i32 comparison form; no invalid artifact is accepted. Final native7 passed, /private/tmp/suss-native-protocols-focused-final.log,47705 terminal0.
- Expanded native9 includes strict decoded24-case source corpus and complete arity replacement/receiver-changing recur. Surrounding native core13/session33/dynamic10/Info12/exceptions10/predicates6, compiler pipeline17/runtimeABI14 and auxiliary focused tests all passed, /private/tmp/suss-native-protocols-expanded.log,69519 terminal0. Existing source397/manual/legacy evidence unchanged.
- Python66, inventory1065, unchanged overlay27+1038 and exact eight-file core artifact check passed; review documentation/manifest will be updated after public native helper validation.
- Two additional actual runtimeABI ownership/foreign raw-environment regressions are building as handle55938, /private/tmp/suss-native-protocols-properties-expanded.log; this exact Cargo graph remains live and must not overlap native/oracle graphs. No outcome claimed yet.

Next finish new ABI tests, expose and test the core native-satisfies? helper used by
upstream foundations, rerun expanded fresh corpus and focused tests, update reviewed
provenance/docs, require full workspace baseline, publish and dispatch independent
review/fixes, then require exact final-head CI. Continue complete collection/core
bootstrap and remaining M2–M9; no milestone or full ROADMAP completion claim.

## Expanded native protocol reference and guards — 2026-09-30

ABI ownership/foreign raw-environment graph55938 completed successfully (ABI16),
/private/tmp/suss-native-protocols-properties-expanded.log. Public native-satisfies?
is a first-class two-argument resident core function; the satisfies? native fallback
reads its current binding, while the syntactic direct marker fast path remains.
Public focused graph13984 passed native9/session33/pipeline17/ABI16,
/private/tmp/suss-native-protocols-public-focused.log.

Fresh expanded primary graph64062 failed comparison for one newly added, previously
unvalidated arity expectation: a valid protocol call selecting a plain single-arity
native implementation ignores surplus arguments and returns Number3, rather than
throwing. The pinned implementation also fills missing parameters with undefined.
Corrected that new expectation and the native call adapter; the original22 verified
observations remain unchanged. No oracle result was skipped or replaced with success.
Final fresh29 primary plus independently decoded native10 passed graph92316,
/private/tmp/suss-native-protocols-oracle-final.log. Reproduce with
`CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target sh scripts/test-native-protocol-oracle.sh`.

Final surrounding focus graph8287 passed native10/core13/session33/nominal3/
pipeline17/ABI16, /private/tmp/suss-native-protocols-final-focus.log. Additional
private-helper wrong-schema/cell/Args/owner guards passed native10/ABI16 graph95567,
/private/tmp/suss-native-protocols-guards.log; typed language exceptions are checked
before storage access. Python66, inventory1065, review overlay28+1037, exact eight
core artifacts and offline roadmap preview passed. Source imports remain the same
four forms; only their manifest's review-overlay hash changed. No ABI prelude,
numeric helper, shipped dependency, issue acceptance status or milestone changed.

Required full baseline passed: `CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2 cargo test --workspace --locked -- --test-threads=2`, graph20021 terminal0, /private/tmp/suss-native-protocols-baseline.log. No native or fresh oracle graph remains live. Next publish the native foundation PR against main, dispatch independent review/fixes, and require exact final-head CI. Then continue source-backed sequence/collection foundations and the remaining M2–M9 acceptance; the full ROADMAP goal remains active.

## Independent PR #77 review — 2026-09-30

Reviewed candidate c9806c9 against accepted design, source provenance and pinned
ClojureScript native dispatch expansions. No significant production defect was
found in the supported source surface. Direct descriptor methods precede live
specific/default tables; the current binding cell is read during native fallback,
not protocol construction. Closure ownership preserves original callback
arguments, avoids a globally rooted registry and retains raw foreign environments.
GlobalCell HIR/IR type guards and private runtime argument/schema checks were
inspected and executed. Arbitrary native host properties remain a documented
unsupported boundary rather than a claim of full core compatibility.

Added an independent regression for qualified native extension from a different
namespace, captured dispatch, current method replacement and re-extension after
forced GC. Native11 passes; initial ABI16/nominal3 focus also passes.
Fresh pinned native29 observations match exactly and native11 passes via
`CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2 sh scripts/test-native-protocol-oracle.sh`.
Review logs: /private/tmp/suss-pr77-review-focus.log,
/private/tmp/suss-pr77-review-native.log and /private/tmp/suss-pr77-review-oracle.log.
Required final review workspace baseline is recorded below upon completion.
No issue acceptance or milestone status changes; next require exact reviewed-head
CI before ready, then continue sequence/collection/core foundations. Do not merge.

Review required full baseline completed successfully: graph74362 terminal0,
/private/tmp/suss-pr77-review-baseline.log, command
`CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2 cargo test --workspace --locked -- --test-threads=2`.
Python66, pinned inventory1065, review overlay28+1037 and exact eight core-import
artifacts also pass. All review native/reference graphs are terminal and released.

## PR77 review and array reference preparation — 2026-09-30

Previous goal turn made concrete progress: full native protocol baseline passed,
implementation committed/pushed c9806c9 and draft PR77 opened against main with
Refs #11. Independent review dispatched as /root/review_pr77. Reviewer found no
significant production defect and pushed qualified-namespace/live-cell/GC regression
and evidence at1022fdafa8a1eff8f71f3d9455d5defddf7101d2. Native11/ABI16/nominal3,
fresh29, Python66/inventory1065/reviews28+1037/artifact8 and required full workspace
baseline passed. Reviewer explicitly released all terminal local native/reference
graphs. Root fast-forwarded the native worktree; final-head CI36752018955 is live
for exact1022fda. PR77 remains draft until that actual run passes; no merge.

Prepared isolated /private/tmp/suss-array-foundation, branch
resurrection/portable-array-foundation, based on reviewed1022fda. Its own untracked
preparation was stashed/restored during review-head reconciliation; only that
temporary stash was dropped. Unrelated root files/worktrees/stashes preserved.
Pinned submodule initialized from local reference source. Array declarations/macros
are audited in docs/runtime/arrays.md. No public source arrays are implemented yet.

`CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target sh scripts/test-array-oracle.sh`
compiled and executed fresh primary40, all exact comparisons passed, then the
Suss missing-feature regression failed with located unresolved alength. Combined
graph67049 terminal101, /private/tmp/suss-array-primary-and-red.log; actual primary
success is separate from the missing-feature failure. Compiler replacement warnings
for the three effect counters are retained in the log, not suppressed.

Expanded focused regression groups cover scalar/identity/missing storage,
mutation/growth/shallow clones, literal/dynamic/nested allocation, first-class/
native protocols and ordered effects/function retention. Required focused command
`CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2 cargo test -p suss-cli --test portable_arrays --locked -- --test-threads=2`
failed all6 as expected on current production code, graph26505 terminal101,
/private/tmp/suss-array-focused-red.log. No skip, baseline weakening or compatibility
claim. Macro make-array literal nil fill differs from dynamic/first-class undefined
holes; source architecture must preserve it. Next implement GC-owned mutable arrays,
source/macro arities and native classification before broader sequence/list/variadic
and persistent collection source porting. Full M2–M9 goal remains active.

## GC-owned array implementation and PR77 readiness — 2026-09-30

Previous goal turn committed verified40 array reference observations and six
missing-feature regressions at92f3f9e9dda124b74099bbcd2d946cdfc855d77a, on the
isolated array branch based on reviewed1022fda. This turn revalidated actual
PR77 final-head CI36752018955: SUCCESS at exact1022fdafa8a1eff8f71f3d9455d5defddf7101d2.
Inspected actual enabled Python66/reviews28+1037/artifact8/native11/ABI16/full
workspace output in /private/tmp/suss-pr77-final-ci.log. Body updated and PR77
marked ready; no merge or issue closure. Reviewer graphs and CI watch are terminal.

Original array runtime now owns mutable element buffers through a private tagged
UserObject, copying Args at construction and preserving owner identity through
growth. Clone ownership is distinct and shallow. One descriptor global appends
without changing the ten-type prelude/version or numeric helper/global indices.
New Array HIR/IR normalizes effectful operands and verifies arity/type/dominance.
Native protocol classification recognizes source arrays before object fallback.
Core array/array?/make-array/aclone/aget/aset/alength factories are resident in each
phase; canonical aliases and old captures use live cells/universal invocation.

First implementation focus60978 failed Rust compilation (pattern bindings,
emission insertion and encoder f64 constant types), /private/tmp/suss-array-first-focus.log.
Corrected construction errors; second focus10269 passed all6 against original40,
/private/tmp/suss-array-second-focus.log. An intermediate order focus26789 failed
a Rust impl lifetime annotation; fixed Analyzer's actual shape before continuing.

Expanded fresh53 comparison85682 failed two newly added, previously unvalidated
expectations: aset evaluates the final value before invalid-target failure, and
an unqualified user alength definition hides the auto-referred macro (both calls
returned42). Actual pinned observations were inspected, new expectations corrected
and eager SetTarget validation removed. The original40 certified observations
remain unchanged. A follow-up script57063 stopped at the same uncorrected comparison
after a local patch script stopped early; no success was claimed. Final expanded54
primary/native7 passed2012, /private/tmp/suss-array-expanded-primary-validated.log.

Two further source-audited multidimensional macro cases retain literal outer-size
ceil/negative-empty behavior, while inner/runtime sizes remain dynamic integers.
Final fresh56 and native9 passed95073, /private/tmp/suss-array-primary-final.log.
Warnings for deliberate effect-counter and alength definitions remain in raw logs.
No primary result was skipped, compared loosely or fabricated.

Guarded surrounding focus80622 passed native7/native-protocol11/session33/pipeline17/
ABI16, /private/tmp/suss-array-guarded-focus-final.log. One local delimiter edit
failed before Cargo execution in /private/tmp/suss-array-guarded-focus.log; fixed
before that successful rerun. Storage/GC/compiler negative focus21530 then passed
native8/compiler-array1/ABI18, /private/tmp/suss-array-storage-focus.log. Final
bounded focus56721 passed native9/compiler-array1/ABI18,
/private/tmp/suss-array-bounded-focus.log. Independent low-level guards cover wrong
Args/owners/indices/dimensions/fill values and malformed owner storage; all produce
language exceptions rather than traps. Args isolation, GC-owned stored Numbers,
growth/clone identity and retained functions are executed, not encoding-only.

Bootstrap allocation/growth caps1,000,000 elements; multidimensional total cells
also cap1,000,000, dimensions buffer64. Typed diagnostics precede allocation and
invalid casts/accesses. Empty outer dimensions do not validate unused leaves.
Named/coerced host keys, negative/fractional property writes, string access via
array macros, checked-array options, source literals and full compiled core/macros
remain unfinished. None of these are claimed as exclusions/completed compatibility.

Python66/inventory1065/review overlay40+1025/exact eight source-core artifacts and
offline roadmap preview passed45561. Twelve runtime/macro source declarations have
partial hash-bound provenance reviews; no source form copied or shipped dependency
added. Existing four source imports and license bytes remain unchanged; only
manifest review hash changes. Full required workspace baseline runs alone as52701,
/private/tmp/suss-array-baseline.log; await terminal before publication. No other
local Cargo/reference graph is live. Next publish partial array foundation with
Refs #9/#11/#17, dispatch independent review/pushed fixes and require exact final-head
CI. Then continue source-backed sequence/list/variadic rest/apply foundations and
the remaining M2–M9 acceptance. Full ROADMAP goal remains active.

Required full workspace baseline52701 completed terminal0, /private/tmp/suss-array-baseline.log: `CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2 cargo test --workspace --locked -- --test-threads=2`. All local native/reference graphs are terminal. Complete source/provenance/manifest/diff checks passed before publication. No RUSTFLAGS override or unrelated deletions. Publish the partial array foundation PR and dispatch independent review; final-head CI remains required.

## Independent PR #78 review — 2026-09-30

Reviewed candidate ef129249 against accepted design, partial compatibility reviews,
actual Wasm storage/IR and pinned array runtime/macro declarations. No significant
production defect was found in the documented supported source surface. Private
source-array descriptor identity keeps Args separate; construction copies Args,
growth retains the same owner, and shallow clones own separate storage while
retaining shared element values. Traversal/evaluation order, macro literal fill,
dynamic sizes, multidimensional resource guards and malformed-storage exceptions
were inspected. General host keys/string access, checked-array options and full
source collection integration remain unfinished; no acceptance status changed.

Added an independent alias-qualified macro regression across namespaces, dropped
original var roots, forced GC, alias-preserving growth, independent shallow-clone
length, shared nested mutation and user runtime alength hiding only unqualified
macro lookup. Corrected the resolver's stale explanatory comment accordingly.
Focused native10/compiler-array1/ABI18 passed graph86982 terminal0,
/private/tmp/suss-pr78-review-focus.log. Fresh pinned primary56 comparisons and
native10 passed graph18776 terminal0, /private/tmp/suss-pr78-review-oracle.log.
Required full workspace baseline is recorded below upon terminal completion.
No merge or issue closure. Final-head CI remains required before readiness.

Review required full baseline completed successfully: graph72602 terminal0,
/private/tmp/suss-pr78-review-baseline.log, command
`CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2 cargo test --workspace --locked -- --test-threads=2`.
Python66, pinned inventory1065, review overlay40+1025 and exact eight core-import
artifacts also pass. All review Cargo and primary reference graphs are terminal.
Next require exact reviewed-head CI, then continue source-backed sequence/list and
variadic rest/apply foundations. Full M2–M9 goal remains active.

## Mutable nominal fields — primary evidence and native regression

The next source sequence prerequisites use mutable fields: List/Cons hash caches,
LazySeq realization and IndexedSeqIterator position. Added 28 independently encoded
source cases covering all three pinned mutability flags, metadata map/type hints,
assignment return values, alias/captured reader/setter lifetime across GC, undefined
constructor fields, RHS throws/effects and the physical receiver across recur.
Fresh pinned reference run57892 completed: all28 observations match exactly.
The same native corpus fails as expected on unsupported type field attributes
(terminal101), /private/tmp/suss-mutable-field-primary-and-red.log. Command:
`CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target sh scripts/test-mutable-field-oracle.sh`.
No implementation or acceptance status changed in this preparation. Next implement
checked scoped mutable-field assignment, preserve immutable/local rejection and
receiver anchoring, then run focused and full acceptance before publication/review.
No local Cargo or fresh reference graph remains live. Full M2–M9 remains active.

## Mutable-field implementation validation — 2026-09-30

Original HIR FieldSet lowers through the already existing checked runtime setter;
no runtime helper/layout/global/ABI changes. Scoped field metadata records the three
pinned mutability flags and ignores type hints at runtime; immutable and shadowing
locals reject. Physical receiver anchoring survives recur and nested closures.
First native corpus run16613 passed28 cases (terminal0). Guarded focus48347 passed
native2, native protocols11, nominal compiler4 and ABI18 (terminal0),
/private/tmp/suss-mutable-field-guarded-focus.log. Fresh primary43447 passed all28
exact observations and native2 (terminal0), /private/tmp/suss-mutable-field-primary-final.log.
Initial full baseline17141 failed one stale negative atomicity case that expected
mutable metadata to reject; replaced that case with an explicitly unsupported
attribute, preserving compile-error binding publication assertions. Focus14595
passed (terminal0), /private/tmp/suss-mutable-field-atomicity-focus.log. Final full
baseline81267 is recorded below only upon terminal completion. Python66, inventory1065,
review overlay40+1025, exact eight source artifacts, numeric manifest, WIT lock and
offline roadmap preview pass. No certified oracle expectations changed or skips
added. Mutable/nominal/list/core issue acceptance remains incomplete.

PR #78's reviewed headb364a2a CI36759096530 passed; actual enabled-suite log was
inspected at /private/tmp/suss-pr78-final-ci.log. Independent review/pushed regression
evidence is in its PR body; #78 marked ready without merge. #77 remains unmerged.
Next publish scoped mutable fields as partial Refs #9/#11/#17, dispatch independent
review/pushed significant fixes and require exact final-head CI before readiness.
Then continue source sequence/list, variadic rest/apply and remaining M2–M9.

Required full mutable-field baseline81267 completed terminal0:
`CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2 cargo test --workspace --locked -- --test-threads=2`,
/private/tmp/suss-mutable-field-baseline-final.log. All local Cargo/primary graphs
are terminal; no RUSTFLAGS override or unrelated deletion. Publish for independent
review; exact reviewed-head CI remains required and no merge is authorized.


## Independent PR #79 review — 2026-09-30

Reviewed fa97d6d against the accepted contract and pinned analyzer field metadata,
set! checks and physical method receiver behavior. No significant production
finding in the supported surface. Checked setter reuses existing owned Args and
bounds/schema guards; RHS values are emitted once and returned without reevaluation.
Nested closure capture and lexical scope restoration retain the physical receiver.
Other field attributes and general host property assignment remain unfinished.

Added four fresh pinned/native regression observations for outer repeated metadata
precedence, truthy zero mutability, independently retained alternate flags and RHS
nested lexical shadow restoration. Added compiler rejection for outer false metadata
overriding an inner true shorthand. All32 primary observations match exactly and
native2/compiler4 pass. Focus85034 and fresh oracle32776 are terminal0:
/private/tmp/suss-pr79-review-focus.log and /private/tmp/suss-pr79-review-oracle.log.
Commands: `CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2 cargo test -p suss-cli --test portable_mutable_fields -p suss-compile --test portable_nominal --locked -- --test-threads=2`
and `CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2 sh scripts/test-mutable-field-oracle.sh`.
Python66/inventory1065/review overlay40+1025/exact eight core-import artifacts pass.
Workspace-wide formatting check reports pre-existing unrelated drift; only the
changed review compiler test was formatted. No broad formatting edit or skipped test.
Required full baseline result is recorded below upon terminal completion. Next
require exact reviewed-head CI, then continue source-backed sequence/list and
variadic rest/apply foundations. Full M2–M9 remains active; no merge/issue closure.


Review required full baseline96773 completed terminal0:
`CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2 cargo test --workspace --locked -- --test-threads=2`,
/private/tmp/suss-pr79-review-baseline.log. All review Cargo/fresh reference graphs
are terminal and released. Push this review regression/evidence commit and require
its exact final-head CI before readiness. No merge is authorized.

## inc/dec source prerequisites — primary evidence and native failures

Prepared35 new source-import cases (107 total, original72 unchanged) for captured
first-class inc/dec over scalar coercions, UTF-16 concatenation, binary64 rounding,
canonical aliases, arithmetic/runtime-var redefinitions and once-only operands.
Fresh pinned graph76820 matched all107 exactly; native corpus plus two new focused
tests fail unresolved inc/dec (terminal101), /private/tmp/suss-inc-dec-primary-and-red.log.
No source port or completion status changed in this preparation. Existing12 import
tests pass. Next retain original pinned forms/notices, add explicit defn patches
and partial hash-bound reviews, regenerate artifacts and validate execution.
This worktree includes independently reviewed mutable head01785e4; all local
reference/Cargo graphs are terminal and root owns the next graph. Full M2–M9 remains active.

## inc/dec source imports — 2026-09-30

Retain pinned inc1505 and dec2803 source forms/docstrings/notices with explicit
fixed defn patches and byte-preserved EPL packaging. Original arithmetic bodies
remain; fresh primary and emitted core.js confirm captured primitives ignore
public arithmetic redefinitions. Six forms now generate ten deterministic artifact
files; two partial hash-bound reviews bring overlay42/1023. No compiler/runtime
implementation, ABI/layout/global/helper/dependency change. Full source macros,
metadata/privacy, object conversion and automatic core loading remain unfinished.

Initial native graph66287 passed15 imports (terminal0), /private/tmp/suss-inc-dec-first-focus.log.
Fresh graph84980 passed107 exact primary/native15 (terminal0),
/private/tmp/suss-inc-dec-primary-final.log. Added six NaN/undefined/nonnumeric-string
probes and typed unsupported-object recovery; fresh graph54052 passed113 exact
primary/native16 (terminal0), /private/tmp/suss-inc-dec-expanded-primary.log.
Original72 certified observations unchanged; all41 new expectations verified
independently against the pin, no skipped observations or opaque decoding.
Python66/inventory1065/reviews42+1023/exact ten source artifacts/offline preview pass.
Required full baseline recorded below only after terminal completion.
Next publish partial Refs #9/#16/#17, independent review with pushed significant
fixes, exact final-head CI; no merge. Then continue comparisons/sequence/list,
variadic rest/apply and full M2–M9 acceptance. Full roadmap goal remains active.

Required full inc/dec baseline32414 completed terminal0:
`CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2 cargo test --workspace --locked -- --test-threads=2`,
/private/tmp/suss-inc-dec-baseline.log. All local Cargo/reference graphs terminal.
Final Python66, inventory1065, reviews42+1023, exact ten artifacts, numeric/WIT
locks and offline preview pass. No RUSTFLAGS override or unrelated deletions.
Publish the partial source import and dispatch independent review; final-head CI
remains required before readiness. No merge or issue/milestone closure.

## Independent PR #80 review — 2026-09-30

Reviewed candidate8d184c8 against retained pinned inc/dec forms, arithmetic macros,
emitted reference core.js, hash-bound patches and generated EPL artifacts. No
significant production finding in the documented supported surface. Arithmetic
bodies remain primitive operations independent of mutable public arithmetic vars;
first-class inc concatenates UTF-16 strings whereas dec numerically coerces them.
Source macro bootstrap, object conversion, metadata/privacy and automatic core
loading remain unfinished; no acceptance or milestone status changed.

Added four independent fresh primary/native observations for callee capture before
operand rebinding and thrown operands with finally cleanup. Original113 certified
observations remain unchanged. Native alias-qualified regression also forces GC
and verifies restoration after temporary runtime binding replacement. Focus41191
and fresh pinned graph28719 completed terminal0: native17 and117 exact primary
observations. Logs: /private/tmp/suss-pr80-review-focus.log and
/private/tmp/suss-pr80-review-oracle.log. Deliberate reference redefinition warnings
remain recorded; no skipped or opaque observations.
Commands: `CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2 cargo test -p suss-cli --test core_import --locked -- --test-threads=2`
and `CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2 sh scripts/test-core-import-oracle.sh`.
Python66/inventory1065/reviews42+1023/exact ten source artifacts pass in
/private/tmp/suss-pr80-review-provenance.log. Required full workspace baseline
result is recorded below after terminal completion. Next require exact reviewed
head CI before readiness, then continue source sequences/lists, variadic rest/apply
and remaining M2–M9 acceptance. No merge or issue closure is authorized.

Review required full baseline52286 completed terminal0:
`CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2 cargo test --workspace --locked -- --test-threads=2`,
/private/tmp/suss-pr80-review-baseline.log. All review Cargo and fresh primary
reference graphs are terminal and released. No RUSTFLAGS override or unrelated
file deletions. Push this independent regression/evidence commit and require exact
final-head CI before readiness. Full M2–M9 goal remains active; do not merge.

## Comparison preparation — 2026-09-30

Root comparison worktree /private/tmp/suss-comparison-foundation, branch
resurrection/portable-comparisons, based on independently reviewed inc/dec
24c16ca9a6f0066ab21466dab559123383318125. Reviewer80 released all Cargo/reference
graphs after native17, fresh117 and required full baseline passed. Its exact-head
CI36765282719 remains running; no readiness claimed. PR79 reviewed head01785e4
passed exact CI36762904497 with actual enabled-suite logs inspected at
/private/tmp/suss-pr79-final-ci.log; body updated and marked ready without merge.

Prepared119 shared source comparison cases plus four explicit capture divergences.
Initial123-case graph89629 failed in the primary runner with a captured-core
wrapper TypeError under fixed-arity runtime replacement (terminal1),
/private/tmp/suss-comparison-primary-and-red.log. This reproduces the arithmetic
boundary already documented in docs/runtime/arithmetic-values.md: accepted design
section7 requires retained original captures, while pinned core generic wrappers
read replacement var arity properties. It is not a new semantics decision.
All four original probes remain in a separate strict catalog with exact pinned
error name/message and separate expected accepted-native behavior; neither outcome
is claimed equivalent, hidden, skipped or replaced by an unknown success.

Fresh graph3385 verified119 exact primary observations and four captured TypeError
classes, then native2 failed unresolved< (terminal101),
/private/tmp/suss-comparison-primary-and-native-red.log. Fresh graph34513 additionally
verified the exact four error name/messages, then native2 failed unresolved<
(terminal101), /private/tmp/suss-comparison-documented-captures-and-red.log.
Focused graph67189 compiled all three native regressions and failed as expected
only on unresolved comparison names (terminal101),
/private/tmp/suss-comparison-focused-red.log. Offline strict comparison verification
also passes after exact pin/schema/capture-catalog guards. All local graphs terminal.

Primary evidence establishes scalar/UTF-16 relational order, strict macro identity,
unary operand erasure versus runtime evaluation, repeated middle macro operands,
short-circuit chains, qualified/runtime replacement and lexical shadowing.
No comparison implementation, copied form, declaration status or acceptance gate
changed yet; see docs/runtime/comparisons.md. Next add original checked binary
comparison runtime primitives, canonical first-class functions and verified HIR/IR
with bounded macro expansion preserving these observations. Keep the four accepted
old-capture divergences explicit. Then focused/fresh/full baseline, partial source
reviews/provenance, PR with Refs #9/#11/#17, independent review/pushed significant
fixes and exact final-head CI; no merge. Complete sequence/list, variadic rest/apply
and all remaining M2–M9 acceptance remain the full active goal.

## Comparison implementation and merged stack — 2026-09-30

User merged PRs77–80; GitHub verifies all four MERGED with no closing issue
references, appropriate for their partial acceptance work. Main055282f9a2be9d7f63e25cb9c91b9bebffb38eb0
has exactly the independently reviewed PR80 tree24c16ca. Its final CI36765282719
passed; actual enabled-suite logs are /private/tmp/suss-pr80-final-ci.log.
Comparison preparation was rebased onto this main using explicit --onto24c16ca;
an initial default rebase replayed squash-merged ancestors and was aborted.
Own temporary autostashes were restored; unrelated user work was preserved.

Original comparison helpers now implement UTF-16 string ordering, supported scalar
numeric coercion and strict identity, with five canonical first-class functions.
Checked HIR/IR binary operations and bounded macro expansion preserve unary operand
erasure, repeated middle syntax and short-circuit evaluation. Macro expansion is
limited to256 operands; a runtime300-argument call is verified separately.
There are29 bootstrap cells, no new globals/layout/version or shipped dependency.
Public numeric== is limited to the documented primitive domain; full -equiv hook
and redefinition, object coercion, compiled source macros and source protocols are
unfinished. No issue or milestone acceptance is claimed.

Focused red67189 failed unresolved comparisons before implementation. First
implementation focus84282 passed. Guarded80892 exposed an invalid new namespace
fixture combining explicit refer and own declaration; corrected the fixture and
kept the established ambiguity rejection. Guarded9363 passed native5/compiler1,
session33/pipeline17/ABI18. Bounded focus11296 passed native6/compiler2.
Logs: /private/tmp/suss-comparison-focused-red.log,
/private/tmp/suss-comparison-first-focus.log,
/private/tmp/suss-comparison-guarded-focus-final.log,
/private/tmp/suss-comparison-bounded-focus.log. All graphs terminal.

Fresh pinned graph47440 passed119 unchanged shared observations, five explicit
capture-wrapper divergences and native6 (terminal0),
/private/tmp/suss-comparison-five-captures-final.log. Numeric== adds the fifth
exact reference TypeError/native retained-capture observation; the other four
and119 shared expectations remain unchanged. These are119 matches plus five
documented differences, never124 equivalences. Strict comparison parser tests
reject Boolean schema values, malformed tags, dropped/diverted observations,
duplicate cases and changed error text. Python70 passed.
Command: `CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2 sh scripts/test-comparison-oracle.sh`.

Required full baseline37118 completed terminal0:
`CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2 cargo test --workspace --locked -- --test-threads=2`,
/private/tmp/suss-comparison-baseline.log. Existing manual legacy ignores and
diagnostic differential outcomes are unchanged. Final provenance27995 passed
Python70/inventory1065/reviews52+1013/exact ten core artifacts/WIT15files6packages,
numeric locks and offline preview10milestones39issues. No RUSTFLAGS override,
unrelated deletion or acceptance change. All Cargo/reference graphs released.

Next publish comparison foundation with Refs #9/#11/#17, dispatch independent
review, push significant fixes and require exact final-head CI before readiness.
Do not merge. Then continue source sequence/list, variadic rest/apply and remaining
M2–M9 acceptance, including the deferred public -equiv integration.

## Independent PR #82 review — 2026-09-30

Reviewed candidate2314fcc against pinned comparison runtime/macro forms and
accepted scalar/UTF-16/evaluation-order/retained-capture contracts in isolated
/private/tmp/suss-review-pr82. Significant finding: an explicit :refer of a user
<, <=, >, >= or == incorrectly selected the automatic core comparison macro,
returning a Boolean instead of invoking the referred function. An actual source
module regression failed before the fix (/private/tmp/suss-pr82-review-refers-red.log,
terminal101); bootstrap lookup now respects those explicit user refers. The fixed
regression covers all five names, qualified aliases, forced GC and explicit core
qualification. The initial fixture attempted to require an unsourced namespace;
corrected it to a real source file before reproducing the semantic failure.

Added four independent thrown-operand/finally/short-circuit observations to the
primary/native corpus; original119 observations and all five explicit captured
wrapper divergences are unchanged. Fresh pinned graph65259 completed terminal0:
123 exact observations plus five separately reproduced TypeErrors and native7,
/private/tmp/suss-pr82-review-oracle-final.log. Focus graph3095 passed native7 and
compiler2, /private/tmp/suss-pr82-review-refers-fixed.log. Commands:
`CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2 cargo test -p suss-cli --test portable_comparisons -p suss-compile --test portable_comparisons --locked -- --test-threads=2`
and `CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2 sh scripts/test-comparison-oracle.sh`.
Python/provenance72269 completed terminal0:70 tests/inventory1065/reviews52+1013,
exact ten source artifacts/WIT locks/numeric manifest/offline preview,
/private/tmp/suss-pr82-review-provenance.log. Public -equiv hooks, arbitrary object
coercion and source macro/core integration remain explicitly unfinished. No issue
or milestone acceptance is inferred. GitHub issues #9 and #10 are already closed;
remaining implementation gaps are separate from those user-controlled live states.

Required full workspace baseline result is recorded below after completion. Next
push this review fix/evidence, require exact final-head CI before readiness, then
continue source sequence/list, variadic rest/apply and remaining M2–M9 acceptance.
Do not merge.

Review full baseline89349 completed terminal0:
`CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2 cargo test --workspace --locked -- --test-threads=2`,
/private/tmp/suss-pr82-review-baseline.log. All Cargo/reference graphs are terminal
and released. Existing manual ignores/differential9pass7fail remain explicit;
no RUSTFLAGS override, unrelated file deletion or acceptance change. The original
119 case dictionaries and five-divergence catalog were independently checked
unchanged. Push the review commit and require final-head CI before readiness.

## PR #82 follow-up pinned namespace audit — 2026-09-30

The earlier independent review's explicit-ref shadowing claim and fix are
WITHDRAWN: they inferred behavior from own declarations without executing the
corresponding pinned namespace source. Root's implements? oracle exposed that
assumption. Fresh original provider/referrer comparison sources compiled and
executed at the pinned ClojureScript commit (graph80524 terminal0) returned
[true,77,true,78,false,79,false,80,false,81]: unqualified explicit user refers
still select automatic comparison macros; alias-qualified calls invoke user
functions. /private/tmp/suss-pr82-referral-audit.log records actual output and
expected redefinition warnings. Initial55241 failed namespace lookup because the
temporary fixture was written under a wrong relative output path; corrected the
fixture before the authoritative compilation.

A shared ten-case tagged referral corpus and original development-only provider
now reproduce that distinction through actual pinned artifacts and independently
decoded native execution. The strict oracle includes a mandatory referrals field;
dropped or replaced observations fail. Regression84012 failed against reviewed
head634c368 (terminal101), /private/tmp/suss-pr82-pinned-refers-red.log. Removed
the incorrect lookup guard and replaced the inferred regression with the certified
one. Focus88641 passed native7/compiler2 (terminal0),
/private/tmp/suss-pr82-pinned-refers-fixed.log. Original123 shared cases and five
explicit captured-wrapper divergences are preserved. This corrects a review-introduced
regression; it is not a new semantics decision or an acceptance claim.

Follow-up fresh70911 completed terminal0:123 unchanged shared observations,
five unchanged explicit divergences and ten certified referred/aliased values,
native7, /private/tmp/suss-pr82-referral-oracle-final.log. Command:
`CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2 sh scripts/test-comparison-oracle.sh`.
Python/provenance61921 completed terminal0:71 tests/inventory1065/reviews52+1013,
exact ten artifacts/WIT/numeric/offline preview,
/private/tmp/suss-pr82-referral-provenance.log.
Required full follow-up baseline53868 completed terminal0:
`CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2 cargo test --workspace --locked -- --test-threads=2`,
/private/tmp/suss-pr82-referral-baseline.log. All Cargo/JVM/Node graphs terminal
and released. No RUSTFLAGS override, hidden failures or unrelated deletions.
Public -equiv, arbitrary object coercion and complete source macro/core integration
remain pending. Push this correction and require its exact-head CI before readiness;
next source sequence/list, variadic rest/apply and remaining M2–M9 acceptance.
No merge or issue/milestone acceptance is claimed.

## Direct implementation predicate prerequisite — 2026-09-30

Current worktree /private/tmp/suss-implements-foundation, branch
resurrection/portable-implements, based on independently reviewed comparison
634c368. GitHub82 remains OPEN/ready with exact CI36774744617 SUCCESS; no agent
merge. Root's separate local sequence/list preparation is
7a99cfd579ab2dc36d31a99b34f961e84af71849 at
/private/tmp/suss-sequence-list-foundation:48 certified reference observations and
native unresolved-seq red, with verified historical issue10/PR64 merge tracking.
Verify its live branch rather than inferring native support from the preparation.
All prior native/reference graphs released before starting this prerequisite.

Source seq/first/rest/next require direct-only implements? distinct from native
satisfies?. Initial fresh graph89427 certified24 exact independently encoded
observations then actual native regression failed unresolved Runtime implements?
(terminal101), /private/tmp/suss-implements-primary-and-native-red.log. Original
bootstrap lowering reuses checked Nominal::Satisfies/direct marker storage without
native/default tables, new runtime globals/helpers/layout/cells/version or copied
source. It resolves protocol names syntactically with phase-local stable keys;
lexical/own/referred user vars hide the macro, aliases/core qualification/exclusions
remain separate. First-class macro values and malformed names/arities are located
errors. Full source core/masks/prototype/metadata and compiled macros remain partial.

First focus22429 passed native1/compiler4 (terminal0),
/private/tmp/suss-implements-first-focus.log. Expanded fresh32557 passed27 exact
primary/native3 (terminal0), /private/tmp/suss-implements-primary-final.log:
original24 retained, three added throw/finally and own-runtime-var/qualified probes.
Reference protocol/redefinition/JS-keyword namespace warnings remain in logs;
no skipped/opaque success. Guarded38567 passed native3, native protocols11,
comparisons7/compiler comparisons2, nominal compiler5 and ABI18 (terminal0),
/private/tmp/suss-implements-guarded-focus.log. Compiler additions verify actual
artifact manifests/phase key isolation and located malformed-call diagnostics.
Native additions cover sourced user refers/core aliases/exclusions, retained
objects/functions after forced GC and language throw recovery.

Commands: `CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2 sh scripts/test-implements-oracle.sh`
and `CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2 cargo test -p suss-cli --test portable_implements --test portable_native_protocols --test portable_comparisons -p suss-compile --test portable_nominal --test runtime_abi --locked -- --test-threads=2`.
Python70/inventory1065/reviews53 partial+1012 unassessed/exact ten core artifacts,
WIT15files6packages/numeric locks/offline preview10milestones39issues pass,
/private/tmp/suss-implements-provenance.log. Required full graph81049 completed terminal0:
`CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2 cargo test --workspace --locked -- --test-threads=2`,
/private/tmp/suss-implements-baseline.log. Existing manual/legacy ignores and
differential9pass7fail remain explicit. Final reference namespace probe92344
is running; record its outcome only after authoritative completion. No RUSTFLAGS,
unrelated file deletion, issue/milestone closure or readiness claim. Next publish
partial Refs #11/#17, dispatch independent review with pushed significant fixes,
require exact final-head CI, and continue source-backed sequences/lists and proper
variadic rest/apply. Full M2–M9 remains active; do not merge.

Supplementary namespace graph92344 failed primary compilation with a macro
expander NullPointerException before observations/native execution (terminal1),
/private/tmp/suss-implements-refers-primary-final.log. The explicit runtime user
refer still selects the automatic implements? macro, so a nil protocol operand
is invalid. This contradicted the inferred refer-hides-macro candidate behavior.
Root removed implements? from the comparison-specific refer-hiding guard and
changed the previously uncertified refer fixture to match direct marker selection;
qualified provider aliases still invoke the function. Original27 certified corpus
expectations unchanged. A proper declared protocol/type replaces nil in the
supplementary pinned namespace fixture. Two additional syntactic protocol name
hygiene probes are prepared (29 total), not yet freshly certified.

This raised uncertainty about PR82's comparison explicit-refer fix. Root returned
PR82 to draft and dispatched its independent reviewer for fresh provider/referrer
primary artifact audit; reviewer exclusively owns all Cargo/JVM/Node graphs until
release. Do not infer comparison behavior from implements? or own declarations.
The previous exact CI634c368 is green but no longer sufficient to claim readiness
until the new semantic uncertainty is resolved. Root makes only isolated edits
while audit runs. Focused/fresh/full checks must repeat after any verified repair.

After reviewer release, root rebased onto corrected PR82 head3d2a4d5. Autostash
0205f6de63d937b2ec91ed57dd82a79fb12a9fcd restored code but two append-only evidence
docs conflicted; resolved by retaining both histories. Automatic approval review
rejected deleting that stash before restoration verification because local work
could be lost; stash is retained untouched. All unrelated stashes/files preserved.

Corrected fresh root graph14074 completed terminal0:29 exact observations/native3,
/private/tmp/suss-implements-corrected-primary.log. Original27 unchanged, two added
syntactic-name/local shadow and runtime-protocol-alias probes freshly certified.
The proper declared-protocol provider/referrer fixture passes before JSON output:
unqualified explicit refer keeps automatic macro, qualified provider alias calls
runtime function. The earlier nil-protocol fixture failed primary macro expansion,
not a compatibility success. Required corrected full baseline is now running;
record only its terminal result before publication. PR82 corrected exact CI36779310477
is running at3d2a4d5, not ready yet. No merge or issue closure claimed.

Corrected required full graph33751 completed terminal0:
`CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2 cargo test --workspace --locked -- --test-threads=2`,
/private/tmp/suss-implements-corrected-baseline.log. All local Cargo/JVM/Node graphs
terminal and released. Final Python71/inventory1065/reviews53+1012/artifacts10,
WIT/numeric locks/offline preview pass; /private/tmp/suss-implements-corrected-provenance.log.
Original27 shared observations remain unchanged; corrected29 total and separate
namespace fixture passed with native3. Publish stacked draft based on corrected
comparison3d2a4d5, partial Refs #11/#17. Dispatch independent review, push significant
findings and require exact final-head CI. Full source sequence/list/rest/apply and
M2–M9 acceptance remain incomplete. Do not merge; retained recovery stash0205f6d
is not removed, and user stashes/files remain untouched.


## Independent PR #83 review — 2026-09-30

Reviewed candidate 66730bc against corrected comparison base 3d2a4d5 in isolated
/private/tmp/suss-review-pr83. Pinned core.cljc 2227–2251 confirms syntactic
protocol resolution with locals removed and direct-only mask/marker tests.
Existing portable stable descriptors correctly preserve the implemented domain;
native/default fallback remains satisfies?. No significant production defect
found. Full source masks/prototypes/metadata/core and compiled macros remain
explicitly unfinished; no issue or milestone acceptance is claimed.

Added four independent observations for direct extension during operand
evaluation, runtime protocol var rebinding without changing the syntactic marker,
finally-returned objects and caught operand throws. Original 29 case dictionaries
are unchanged. Focus 37351 completed terminal 0 (native 3/compiler nominal 5),
/private/tmp/suss-pr83-review-focus.log. Fresh pinned graph 61529 completed
terminal 0:33 exact observations and native 3; the mandatory provider/referrer
fixture also passes before JSON output.
/private/tmp/suss-pr83-review-oracle.log retains actual primary warnings and
results. Command: `CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2 sh scripts/test-implements-oracle.sh`.
Python/provenance 15555 completed terminal 0:71 tests/inventory 1065/reviews 53+1012,
exact ten source artifacts/WIT 15 files/6 packages/numeric manifest and offline
roadmap 10 milestones/39 issues. Required full baseline result follows after its
authoritative completion. No RUSTFLAGS override, unrelated deletion or merge.
Next require exact final-head CI, then source-backed sequence/list dependencies
and variadic rest/apply; complete M2–M9 acceptance remains open.

Review required full baseline 10991 completed terminal 0:
`CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2 cargo test --workspace --locked -- --test-threads=2`,
/private/tmp/suss-pr83-review-baseline.log. Existing manual legacy ignores and
diagnostic differential outcomes remain explicit and unchanged. All review
Cargo/JVM/Node graphs are terminal and released. Push the review observations
and require exact final-head CI before readiness; do not merge.

## Retained core sequence/collection interfaces — 2026-09-30

Worktree /private/tmp/suss-core-sequence-interfaces, branch
portable/core-sequence-interfaces, based on reviewed PR83 e083d8b. The previous
turn verified neutral open PR titles/descriptions but did not change implementation;
this continuation takes the next executable source import step.

Before importing declarations, native graph81606 failed terminal101 on unresolved
cljs.core/ISeqable while loading the existing six-function artifact;
/private/tmp/suss-core-interfaces-native-red.log. Selected fifteen exact retained
runtime defprotocol forms with patch:null, in original source order. Each has an
individual source-hash review and generated-method signature rationale. Original
forms/notices and byte-preserved EPL license files remain packaged. Twenty-one
selected forms now generate25 files; reviews68 in-progress/997 unassessed, not
complete protocol or collection acceptance.

Focus46534 passed core_import17/interface1. Fresh graph25533 completed terminal0:
31 exact pinned scalar observations and native interface2,
/private/tmp/suss-core-interfaces-primary.log. Command:
`CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2 sh scripts/test-core-interface-oracle.sh`.
The original nominal adapter exercises every imported marker/method, both -nth
arities, raw scalar results and identity after GC. It is not a persistent list or
sequence implementation. Fresh existing graph54833 completed terminal0: original
117 observations unchanged/core_import17, /private/tmp/suss-core-interfaces-existing-primary.log,
using the same environment and scripts/test-core-import-oracle.sh.

Provenance99916 completed terminal0: Python71, inventory1065, reviews68+997,
artifacts25, WIT15 files/6 packages, numeric locks and offline roadmap10/39.
Required workspace baseline35101 is running; record terminal result before
publication. No RUSTFLAGS override or concurrent local Cargo/reference graphs.
Next independent PR review and exact final-head CI; then retained sequence/list
source types, UTF-16 indexing and variadic rest/apply dependencies. M2–M9 remain
unfinished; partial PR references only. No PR merge or issue closure claimed.

PR83 exact final-head CI36781755677 succeeded at e083d8b. Root inspected actual
provenance/workspace logs /private/tmp/suss-pr83-final-ci.log and marked PR83 ready
without merging. Existing branch names remain stable; new branches/PRs/commits
use neutral wording. The earlier recovery stash0205f6d remains retained; no stash
or unrelated file deletion is part of this import.

Required full baseline35101 completed terminal0 with the exact locked workspace
command and two test threads, /private/tmp/suss-core-interfaces-baseline.log.
Existing manual/legacy ignores and diagnostic differential9pass7fail remain
unchanged. All root Cargo/JVM/Node graphs terminal and slots released. Publish
this isolated import as a stacked draft, Refs #11/#16/#17; independent review and
exact final-head CI remain required before readiness. Future sequence48-case red
preparation is not included in this passing source import branch.


## Independent PR #85 review — 2026-09-30

Reviewed candidate e1f9e93 against main6364495 in isolated
/private/tmp/suss-review-pr85. Fifteen selected whole protocol forms are exact
pinned source with patch:null; source hashes/notices/EPL packaging and generated
25-file manifest verify. Twenty-one forms and68 partial reviews/997 unassessed
remain prerequisites, not persistent collection or compiled macro acceptance.
No significant production defect found. The earlier same-result -nth fixture
did not distinguish arity dispatch, so added four independent observations for
distinct arity results, left-to-right operand effects and a captured method value.
Original31 case dictionaries are unchanged; fresh graph9522 completed terminal0:
35 exact pinned scalar observations/native2,
/private/tmp/suss-pr85-review-interface-oracle.log.
Command: `CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2 sh scripts/test-core-interface-oracle.sh`.

Added a third native lifecycle test: after source reload/forced GC, both canonical
core aliases and captured method values operate on retained instances; invalid
method arities fail and subsequent calls recover. Focus70530 completed terminal0:
core_import17/interfaces3, /private/tmp/suss-pr85-review-focus.log. Existing fresh
graph89239 completed terminal0: original117 exact observations/core_import17,
/private/tmp/suss-pr85-review-existing-oracle.log, using
`CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2 sh scripts/test-core-import-oracle.sh`.
Provenance6747 completed terminal0: Python71/inventory1065/reviews68+997/25artifacts,
WIT15files6packages/numeric locks/offline10milestones39issues,
/private/tmp/suss-pr85-review-provenance.log. Required full baseline follows its
authoritative completion. No RUSTFLAGS, unrelated deletions, acceptance or merge
claim. Next push review coverage, require exact final-head CI, then implement
retained sequence types and UTF-16 indexing/variadic rest/apply dependencies.

Required review full baseline79552 completed terminal0:
`CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2 cargo test --workspace --locked -- --test-threads=2`,
/private/tmp/suss-pr85-review-baseline.log. Existing manual/legacy ignores and
separate diagnostic differential9pass7fail remain explicit. All review local
Cargo/JVM/Node graphs are terminal and released. Push the independent coverage
commit and require its exact final-head CI before readiness. No merge or issue
closure; retained source sequence/collection implementations remain next work.

## UTF-16 indexed storage prerequisite — 2026-09-30

Isolated /private/tmp/suss-indexed-string-storage, portable/indexed-string-storage,
now based on independently reviewed PR85 847fca7. Root prepared fixtures while
reviewer owned local graphs, then waited for explicit release. Initial fresh
75334 completed terminal101:21 primary matches followed by native language error
on astral string length; /private/tmp/suss-indexed-string-primary-and-red.log.
Original Rust adaptation extends the existing length/get helpers to UTF-16 owners
with guarded numeric access, one-unit strings and internal undefined for missing
keys. No helper/global/type index or ABI version/layout change.

First focus63221 terminal101 exposed an unrelated test-only inc lookup after all
preceding string cases; replaced that probe with primitive addition and freshly
recertified21 in graph75712 terminal0. Expanded graph67199 terminal0 certifies25
observations/native1, adding retained owner/functions across public rebinding/GC
and qualified macro behavior; /private/tmp/suss-indexed-string-expanded-primary.log.
Command: `CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2 sh scripts/test-indexed-string-oracle.sh`.
Original four aget/alength reviews are extended; status/counts remain68 in-progress,
997 unassessed. Regenerated25 artifacts differ only in review manifest hash.
Focus22025 terminal0: native strings1/arrays10/interfaces3, compiler arrays1/ABI18;
/private/tmp/suss-indexed-string-focus.log. Fresh array graph44231 terminal0:
56 original primary observations unchanged/native10,
/private/tmp/suss-indexed-string-array-primary.log. Provenance60098 terminal0:
Python71/inventory1065/reviews68+997/artifacts25/WIT/numeric/offline preview.
Required full baseline65743 is live; record terminal result before publishing.
No RUSTFLAGS override or overlapping reference/Cargo graph. String writes, host
named/coerced property keys, checked-array modes, compiled upstream macros and
full source sequence/core remain unfinished. Next source-backed EmptyList/List/
Cons/IndexedSeq with canonical empty-list publication and rest/apply semantics;
M2–M9 acceptance remains open.

Tracking reconciliation preserves issue IDs: issue10 automatically closed through
merged acceptance PR64 (9ede0d2, Closes #10), while issue9 was manually closed.
Updated stale acceptance/ROADMAP text does not certify remaining compiler gaps.
Source-preparation branch's future48-case unresolved-seq red remains unpublished
and is not reported as a passing implementation. User files/stashes untouched.

Required full baseline65743 completed terminal0 using
`CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2 cargo test --workspace --locked -- --test-threads=2`,
/private/tmp/suss-indexed-string-baseline.log. Existing diagnostic differential
9pass7fail and manual/legacy ignores remain explicit. All root Cargo/JVM/Node
graphs terminal and released. Publish a stacked draft based on reviewed PR85,
Refs #11/#16/#17, then independent review and exact final-head CI. No merge.


## Independent PR #86 review — 2026-09-30

Reviewed candidate88d5b70 against reviewed PR85 847fca7 in isolated
/private/tmp/suss-review-pr86. Existing STRING guards and unsigned bounds preserve
exact UTF-16 units and keep malformed input on language exceptions. No significant
production defect found. Added six independent scalar probes for large unsigned
index boundaries and nested read/owner-throw/finally effects; original25 case
dictionaries unchanged. Fresh graph44907 completed terminal0:31 exact pinned
observations and native corpus1, /private/tmp/suss-pr86-review-oracle.log, using
`CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2 sh scripts/test-indexed-string-oracle.sh`.

Added separate native typed-recovery guards for unsupported host properties,
string writes/clones and wrong runtime arities, followed by lone-surrogate reads
after GC. These are explicit unsupported boundaries, not compatibility claims.
Focus78674 terminal0: strings2/arrays10/interfaces3/compiler arrays1/ABI18,
/private/tmp/suss-pr86-review-focus.log. Existing array graph80274 terminal0:
original56 exact primary observations/native10 unchanged,
/private/tmp/suss-pr86-review-array-oracle.log. Provenance48345 terminal0:
Python71/inventory1065/reviews68+997/artifacts25/WIT/numeric/offline10/39,
/private/tmp/suss-pr86-review-provenance.log. GitHub event/PR audit independently
confirms issue10 automatically closed by acceptance PR64 commit9ede0d2 at
12:48:43Z; issue9 manual closure has no commit at14:19:40Z.

Required full baseline88748 is running; record its authoritative terminal outcome
before publication. No RUSTFLAGS override or overlapping local reference/Cargo
graphs. Full source sequences/core, compiled macros, host property domains and
M2–M9 acceptance remain incomplete. Next push review coverage and require exact
final-head CI, then retained EmptyList/List/Cons/IndexedSeq and rest/apply
semantics. Do not merge or close partial issues.

Required independent full baseline88748 completed terminal0 using
`CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2 cargo test --workspace --locked -- --test-threads=2`,
/private/tmp/suss-pr86-review-baseline.log. Existing manual/legacy ignores and
separate differential9pass7fail remain explicit. All review local Cargo/JVM/Node
graphs are terminal and released. Push independent review coverage and require
exact final-head CI before readiness; no merge or issue closure claimed.

## Retained-source control macro prerequisite — 2026-09-30

Isolated /private/tmp/suss-core-control-flow, portable/core-control-flow, based on
reviewed PR86 52e9fe2. The previous goal turn made implementation progress:15
retained protocols/21 imported forms published in PR85 and UTF-16 indexed storage
published in PR86. Independent review expanded interface35 and string31 primary
observations, with unchanged117 function/56 array corpora and terminal baselines.
Root inspected exact final CI36784876092 at847fca7 and36786314737 at52e9fe2,
/private/tmp/suss-pr85-final-ci.log and /private/tmp/suss-pr86-final-ci.log. Both
PRs marked ready without merging. Their partial issue links remain intact.

Next retained seq/list source needs when/when-not/if-not/and/or/cond. Original
candidate44 observations freshly matched primary execution in graph40798, then
native failed terminal101 on unresolved Runtime and;
/private/tmp/suss-control-flow-primary-and-red.log. Original bounded Rust HIR
expansion uses existing if/do/let IR and fresh binding IDs; it introduces no
runtime helper/global/layout/version. Once-only test evaluation, actual short
circuit values, nominal identity, ordered effects, throws/finally and tail contexts
execute. Literal keyword tests fold to true for conditional branching, without
claiming materialized keyword values. Full compiled macros/metadata/&form/&env/
syntax quote/gensym and binding/destructuring macros remain unfinished.

Initial compile graph32177 terminal101 caught missing HIR binding span/metadata;
fixed those fields. Secondfocus12742 terminal0/native1. Expanded graph48366
terminal0:54 primary matches/native2, original44 expectations unchanged. Command:
`CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2 sh scripts/test-control-flow-oracle.sh`;
/private/tmp/suss-control-flow-expanded-primary.log. Extra cases distinguish own
runtime functions, lexical shadowing and qualified macros; a user not function
cannot change a qualified primitive if-not. Six source-hash reviews retain
inventory reader contexts and record partial bootstrap adaptations, with no
upstream form copied. Overlay74 in-progress/991 unassessed; regenerated25 source
artifacts change only the review manifest hash.

Guarded focus5228 completed terminal0: controls3/interfaces3/comparisons7/
implements3, compiler comparisons2/pipeline17,
/private/tmp/suss-control-flow-guarded-focus.log. Earlier misspecified package/test
selection did not run tests; corrected command selected the actual compiler
pipeline target. Controls include located arity/odd-pair/non-tail/resource errors,
compile atomicity, aliases/exclusions and both phase compilation paths.
Provenance6885 terminal0: Python71/inventory1065/reviews74+991/artifacts25/WIT/
numeric locks/offline10 milestones39 issues. Required baseline83970 is live;
record authoritative terminal result before publishing. No RUSTFLAGS override or
concurrent local Cargo/reference graph. Next canonical empty-list/source types,
hashing/reduction/storage and persistent rest/apply dependencies. M2–M9 remain
unfinished; use partial Refs, independent review and exact final-head CI. No merge.

Required baseline83970 completed terminal0 with
`CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2 cargo test --workspace --locked -- --test-threads=2`,
/private/tmp/suss-control-flow-baseline.log. Existing diagnostic differential
9pass7fail and legacy/manual ignores remain explicit. All root Cargo/JVM/Node
graphs terminal and slots released. Publish stacked draft based on reviewed86;
Refs #11/#14/#16/#17. Require independent subagent review, significant fixes and
exact final-head CI before readiness. No issue/milestone closure or merge.

## Independent PR #87 review — 2026-09-30

Reviewed candidate7a6bdc1 against the accepted design and actual pinned control
macro forms in isolated /private/tmp/suss-review-pr87. Audited checked if/do/let
lowering, hygienic binding identities, field/local/runtime shadowing, phase and
alias resolution, keyword test folding, short circuit values and tail contexts.
No significant production defect found. Corrected the resolver's stale comment
about the scope of runtime definition shadowing.

Five independent shared probes execute callable nominal fields hiding macros,
closure capture through nested temporary bindings, arbitrary keyword truthiness,
selected if-not loop recurrence and thrown cond-test catch/finally effect order.
Original54 observations remain unchanged; fresh graph83012 completed terminal0
with59 exact pinned primary observations and native3 tests, after forced GC.
Command: `CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2 sh scripts/test-control-flow-oracle.sh`;
/private/tmp/suss-pr87-review-oracle.log. Python71, inventory1065, overlay74 reviewed/
991 unassessed and regenerated25 source files verify. Six partial review rationale
counts and manifest overlay hash align with the expanded corpus. No copied source
or new ABI/dependency. Full compiled macro and persistent collection acceptance
remain unfinished; no issue closure or merge.

Required reviewer baseline61320 completed terminal0:
`CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2 cargo test --workspace --locked -- --test-threads=2`,
/private/tmp/suss-pr87-review-baseline.log. Legacy/manual ignores and existing
9pass/7fail diagnostic differential remain explicit. All reviewer Cargo/JVM/Node
graphs terminal, exclusive shared-target slot released to root. Push this review
coverage and require exact reviewed-head CI before readiness; then continue actual
retained sequence/list types and dependencies. No merge or milestone closure.

## Source forward-declaration evidence — 2026-09-30

PR87 independent reviewer pushed2afef8a, fresh59 observations/native3 and required
full61320 terminal0; original54 expectations unchanged. Reviews74+991/artifacts25
and Python71 pass. Root fast-forwarded clean source worktree. Exact reviewed-head
CI36788771234 remains authoritatively live; keep draft until successful enabled
logs are inspected. No merge. Review graph slots explicitly released before the
next reference run.

New isolated /private/tmp/suss-core-forward-declarations, branch
portable/core-forward-declarations based on reviewed87. Pinned source declares
array-seq/prim-seq/IndexedSeq at core.cljs1257, hash-map/list/equiv-sequential1436,
and hash-coll/cons/drop/count/nth/RSeq/List1600. These are real source load-cycle
prerequisites, not optional metadata. Original candidate14 shared scalar probes
freshly match primary in graph84915, then native fails terminal101 on unresolved
Runtime declare; /private/tmp/suss-forward-declaration-primary-and-red.log.

Added an ordinary initializerless definition probe before declare so the existing
source read behavior is independently exposed. Expanded graph44956 terminal101:
15 exact primary observations, then native language error reading the known
initializerless variable; /private/tmp/suss-forward-definition-primary-and-red.log.
Command: `CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2 sh scripts/test-forward-declaration-oracle.sh`.
The reference shows undefined/nil-like/falsey reads, preserving existing values,
mutually referenced fixed functions, live redefinition and defonce initialization
of fresh declarations. Expectations are independently encoded Booleans/binary64,
not unknown-object successes. No semantic implementation or review count changed.

Next repair source-level declared-variable reads and add bounded declare lowering,
while preserving defonce's uninitialized test and the runtime ABI's internal
binding-cell checks. Compiler.cljc855–892 emits no initializer assignment for a
declaration, corroborating the executed reference. Existing HIR Definition None
only declares compiler identity; IR currently emits nil without binding the cell,
and source GlobalRead uses the internal unbound-read error helper. Do not silently
mark an undefined declaration initialized or reset an existing cell. Re-run the
fresh15 and focused definition/session/ABI suites after repair, then required full
baseline and independent PR review/final-head CI. The local red preparation is
unpublished. Actual source EmptyList/List/Cons/IndexedSeq and hashing/reduction/
persistent rest/apply remain unfinished; M2–M9 goal remains active. All reference
and Cargo graphs terminal/released; no RUSTFLAGS override or unrelated deletion.

## Source declaration implementation — 2026-09-30

Previous continuation made concrete progress: added source GlobalRead bound guard
and bounded declare lowering against two prior pinned/native red graphs. This
continuation revalidated worktree/process state. Prior focus31610 handle was
missing after tool context refresh; its completed log showed all four suites
passing, but that was not used as a fresh terminal exit-code claim. Fresh graphs
below provide authoritative results. No surviving reference graph was restarted.

Source GlobalRead now calls existing binding-bound and binding-get for initialized
cells, otherwise returns existing undefined sentinel6 without writing the cell.
Internal binding-get behavior stays unchanged. Initial implementation11965 failed
Wasm validation: binding-bound returns tagged Boolean eqref, not i32; converted
its canonical true sentinel before branching. Corrected12855 exposed fixture's
unloaded boolean function; native test now loads generated core artifact and enters
the primary fixture namespace. Loaded53948 passed native15 then failed the old
compiler test expecting an unbound-read exception after skipped defonce. Fresh
76027 independently established16 exact observations including that skipped-source
read; updated the regression to exact undefined and retained later defonce17 check.

Fresh77202 completed terminal0:17 exact observations/native2, including self-qualified
declaration names. Source-qualified names use existing def namespace checks. Macro
metadata retains original name metadata plus generated declared:true. Expression
results remain explicitly unsupported rather than invented. Metadata runtime
reflection and full compiled macros remain pending. Final focus51507 terminal0:
persistent_session33/forward declarations2/compiler definitions12/ABI18,
/private/tmp/suss-forward-declaration-final-focus.log. Fresh final97430 terminal0:
17 exact primary/native2, /private/tmp/suss-forward-declaration-final-primary.log.
Command: `CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2 sh scripts/test-forward-declaration-oracle.sh`.

One partial source-hash review macro:declare:174 at988d57ef… retains :cljs context;
no upstream form copied. Overlay75 in progress/990 unassessed. Generated25 source
artifacts only change the review manifest hash. Provenance56858 terminal0:
Python71/inventory1065/reviews75+990/artifacts25/WIT15files6packages/numeric/offline
10 milestones39 issues. Required full baseline44806 ended101 on a stale
source-resolution unbound-read expectation;
/private/tmp/suss-forward-declaration-baseline.log. Require authoritative terminal
success before publishing, then independent review/fixes and exact final-head CI.
No RUSTFLAGS override, unrelated deletion, issue closure or PR merge.

PR87 exact reviewed CI36788771234 succeeded at2afef8a. Root inspected enabled
provenance/workspace logs /private/tmp/suss-pr87-final-ci.log, including all three
control tests, and marked it ready without merging. Next actual retained list/
sequence source with canonical empty list, storage, hashing/reduction and persistent
rest/apply; complete M2–M9 acceptance remains unfinished. Source import dependencies
must stay executable and unknown behavior must remain explicit until implemented.


Full baseline44806 exposed the remaining source-resolution expectation. Updated
that test to decode exact undefined6 and unchanged bound flag0, while a separately
constructed direct binding-get fragment preserves its typed language exception,
UTF-16 "Unbound binding" message and later initialized nil behavior. Test wrapper
first failed compilation on private constants, then failed import validation on
its cell-typed parameter; corrected to the existing eqref helper signature.
Fresh focused45811 terminal0: compiler definitions12/resolution11/ABI18.
Required workspace repeat25487 completed terminal0, all enabled suites and
doc-tests passing; /private/tmp/suss-forward-declaration-baseline-repeat.log. No internal ABI behavior or unknown-name resolution was relaxed.

Root validation is complete; next gate is independent PR review, significant
fixes if any, and exact final-head CI. No merge or issue closure is authorized.

## PR88 independent review — 2026-09-30

Reviewed candidate2e6592a independently in /private/tmp/suss-review-pr88 against
PR87 base2afef8a. No significant production defect found within the explicitly
partial declaration contract. Audited declaration namespace/scoping, source order,
metadata, analyzer snapshot atomicity, phase identities, live cell/closure reads,
defonce initialization and the separate direct internal binding-get exception.
Declaration expression results, runtime Var metadata and compiled macros remain
unsupported/pending; this does not complete full core, collections or M2–M9.

Added seven independent primary probes: bound false survives declare/defonce,
old closure reads undefined before initialization then observes the later value,
failed initializer leaves declaration undefined and defonce recovers it, lexical
declare acts as a function, and an own runtime declare hides the automatic macro
while qualified core declare still works. Original17 observations are unchanged;
current24 are exact independently decoded scalar observations with GC between
native cases. Added compiler metadata/source-span/order/duplicate-name and both
phase identity checks, including successful analysis snapshot isolation.

Fresh reference/native graph33546 terminal0:
`CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2 sh scripts/test-forward-declaration-oracle.sh`;
24 exact primary observations, native2, /private/tmp/suss-pr88-review-primary.log.
First metadata test compile95253 ended101 on reviewer private-field access;
corrected to public getters. Focused repeat80808 terminal0: definitions13,
resolution11, ABI18, /private/tmp/suss-pr88-review-focus-repeat.log.
Required full workspace90452 terminal0:
`CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2 cargo test --workspace --locked -- --test-threads=2`;
all enabled tests/doc-tests passed, /private/tmp/suss-pr88-review-baseline.log.
Provenance99543 terminal0: Python71, inventory1065, partial reviews75/unassessed990,
generated25 artifacts, official WIT15 files/6 packages, numeric manifest and
10-milestone/39-issue offline preview. Review count update changes only the
manifest review hash; no source/ABI layout change. No RUSTFLAGS override,
unrelated deletion, issue closure or merge. All local reference/Cargo graphs are
terminal; next gate is exact reviewed-head CI. Next implementation task remains
actual source EmptyList/List/Cons/IndexedSeq with canonical empty list,
hashing/reduction and persistent rest/apply.


## Retained sequence operation protocols — 2026-09-30

Previous goal turn made authoritative progress: PR88 is independently reviewed
atb037290 and ready without merging; exact CI36792256238 passed. Root inspected
enabled provenance/workspace logs /private/tmp/suss-pr88-final-ci.log. Live PR85–88
remain open. Root isolated portable/core-sequence-operations on that reviewed head;
all prior local graphs were terminal. Accepted design remains unchanged.

Pinned List/EmptyList use IStack and IReduce; IndexedSeq also needs IReversible,
IIterable and IDrop. Added25 independent scalar dispatch probes to the existing
35-case interface corpus. First fresh38652 ended101:60 exact primary matches,
then unresolvedIStack/IReduce in two native tests. Upstream warned that extend-type
reduction arities should be grouped; corrected the fixture to its supported grouped
syntax without changing expectations. Fresh41522 ended101:60 exact matches without
that warning, then same two located unresolved declarations; logs
/private/tmp/suss-sequence-operation-primary-red[-grouped].log.

Imported all five whole pinned protocol forms with patch:null, annotations,
docstrings and EPL provenance retained. Source order of all20 protocol declarations
is preserved. The existing defprotocol adapter handles them; no production Rust,
helper/index/global/layout or ABI version changes. Native9847 ended101 only on
the new invalid-arity assertion expecting a message in SessionError Display;
Display deliberately says Uncaught language exception. Require typed Language
exception instead, preserving compile/trap distinction. Fresh67745 terminal0:
60 exact primary observations (original35 unchanged), allfour native interface
tests; /private/tmp/suss-sequence-operation-final-primary.log.
Command: `CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2 sh scripts/test-core-interface-oracle.sh`.

Five hash-bound partial reviews: overlay80/985;26 selected forms produce30 licensed
artifacts. Adapter methods do not establish concrete persistent collections,
reduction traversal, early stopping or actual iterator behavior. Next validate
six-function source/core guards, provenance and required full workspace baseline,
then independent PR review/fixes and exact final-head CI. Actual retained source
EmptyList/List/Cons/IndexedSeq with canonical empty list, hashing/reduction and
persistent rest/apply remains unfinished. No issue closure or PR merge.

Guarded command first ended101 before execution: nonexistent test target
core_source_import; corrected to the actual core_import target.

Guarded54540 terminal0: core_import17/interface4/forward declarations2. Provenance
fresh terminal0: Python71/inventory1065/80 partial985 unassessed/artifacts30/WIT15
files6 packages/numeric/offline10milestones39issues. Original35 JSON observations
are byte-data identical to the parent corpus. Prior21 selected manifest forms
retain every source/form/patch/notice hash and review/dependency field; only seven
extracted filenames shift to preserve source ordering. Full workspace54175 completed
terminal0, all enabled tests and doc-tests passing; /private/tmp/suss-sequence-operation-baseline.log. Require terminal
baseline, independent review and exact final-head CI before readiness.

Root validation is complete for this slice; next gate is independent review,
pushed significant fixes if needed and exact final-head CI. Keep full M2–M9 goal
active; no issue or milestone acceptance is complete from these declarations.

## Independent PR89 review — 2026-09-30

Reviewed candidatebb4db62 against PR88 baseb037290 in isolated
/private/tmp/suss-review-pr89. No significant production defect found within the
partial declaration scope. Independently verified all20 protocol forms are whole,
byte-exact pinned source ranges in source order, with patch:null and EPL notices.
All21 parent manifest entries retain source/form/patch/notice/review/dependency
fields except shifted extracted filenames. No compiler/runtime production change.

Added five shared scalar probes: false initial reduction state, callee capture
before argument rebinding, live extension during argument evaluation, ordered
throw/catch/finally effects and nil-returning drop dispatch. The original60
observations remain unchanged; current65 are independently decoded, with forced
GC between native cases. Focus57012 completed terminal0, four native tests. Fresh
reference/native86823 completed terminal0:65 exact primary matches/native4;
/private/tmp/suss-pr89-review-primary.log. Command:
`CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2 sh scripts/test-core-interface-oracle.sh`.

A fifth native test rejects malformed duplicate/empty/variadic signatures and an
undeclared extension arity, verifies compile atomicity and retained loaded methods
after GC, then valid declaration recovery. Guarded43137 completed terminal0:
core_import17/interfaces5/forward declarations2;
/private/tmp/suss-pr89-review-guarded.log. Python71/inventory1065/80 partial reviews
and985 unassessed/30 source artifacts/WIT15 files6 packages/numeric/offline10
milestones39 issues pass. Five review rationales and current corpus counts align;
regeneration changes only the manifest review hash. Required full reviewer graph17462 completed terminal0, all enabled tests and
doc-tests passing; /private/tmp/suss-pr89-review-baseline.log. Command:
`CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2 cargo test --workspace --locked -- --test-threads=2`.
Existing diagnostic differential9 pass/7 fail and legacy/manual ignores remain
explicit. All local reviewer Cargo/JVM/Node graphs are terminal; shared target
slot released to root.

These original nominal adapters do not establish actual persistent collections,
reduction traversal, reduced-value stopping or iterator implementations. Source
EmptyList/List/Cons/IndexedSeq, canonical empty-list/static-property storage,
hashing/reduction and persistent rest/apply remain unfinished. No RUSTFLAGS
override, unrelated deletion, issue closure or merge. Require final reviewed-head
CI after significant findings/fixes before readiness.


## Named-property prerequisite preparation — 2026-09-30

PR89 independently reviewed09ae837:65 exact primary observations, nativeinterfaces5,
core_import17/forwarddeclarations2, provenance80partial985unassessed/30artifacts
and full17462 terminal0. Root inspected reviewer diff and synchronized the clean
source-import worktree. Exact finalCI36795130080 is running; do not use obsolete
candidateCI36794556208 (cancelled) as readiness evidence. No merge/closure.

Created separate portable/core-named-properties worktree before reviewer slot
release, then fast-forwarded it to09ae837. Prepared22 original scalar probes with
strict independent f64/Boolean decoding. Ran reference/native only after reviewer
confirmed all local Cargo/JVM/Node graphs terminal and released exclusive slot.
First38134 ended1 during primary compilation: static literal nil dot form rejected
by the pin, no observations/native success. Changed that runtime-error probe to
an unknown function parameter receiving nil, and recorded rejected literal form
separately; no unsupported result became a success. Fresh32149 terminal101:
22 exact primary matches, native first case has located unresolved .-EMPTY at
59..66. Intentional ->PropertyProbe replacement warning retained. Log
/private/tmp/suss-named-property-primary-native-red-dynamic.log.
Command: `CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2 sh scripts/test-named-property-oracle.sh`.

No production Rust, ABI or compatibility classification changed in this prep.
Do not publish the red native regression as green acceptance. Next implement
named-property storage/lowering needed by real retained source and validate the
actual artifact. Canonical empty list, Object method blocks, List/EmptyList/Cons/
IndexedSeq and complete hashing/reduction/rest/apply remain unfinished; full
ROADMAP M2–M9 objective stays active. See docs/runtime/named-properties.md.


Root final PR89 gate: exact reviewed09ae837 CI36795130080 completed success.
Root inspected enabled logs /private/tmp/suss-pr89-final-ci.log, including
Python71,80partial985unassessed,30artifacts and allfive interface tests/shared65
observations, plus full workspace results. PR89 was marked ready without merging;
PR description records final run and independent review. The property preparation
remains local and explicitly native-red; next execute its actual implementation,
not claim the preparation as passing acceptance. All local graphs are terminal.


## Named-property implementation — 2026-09-30

Previous goal turn made authoritative progress: PR89 imports five complete source
protocols, independently reviewed09ae837 and ready with inspected finalCI36795130080;
no merge. Its local property preparation78f0646 was a verified native-red regression.
Revalidated this clean worktree and AGENTS/design/ROADMAP/inventory/handoff before
implementation. All prior local graphs terminal; root owns exclusive test slot.

Original checked storage now supports literal identifier .-name reads and set!
for class/function owners and known instance fields, plus actual string/array
length. Closure tables mix UTF-16 name keys with existing native-kind i31 keys0–7;
callback environments, native guard range, ten shared types/globals and ABI version
stay unchanged. Four appended checked helper exports; no owner registry or shipped
JVM/Node. Munged/computed names, extra instance fields, length writes and unsupported
owner shapes stay explicit. Source Object methods, full reflection/metadata and
real persistent collections remain unfinished.

Initial implementation42618 terminal0: original22 observations execute successfully
under actual Wasm validation and GC. Added10 independent probes preserving original22:
native/name entries and extensions, captured environments, nested owner roots,
field named length, dynamic undefined errors and zero/false values. Fresh36222
terminal0:32 exact primary/native2, /private/tmp/suss-named-property-final-primary.log.
Intentional upstream ->PropertyProbe replacement warning remains retained.
Command: `CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2 sh scripts/test-named-property-oracle.sh`.

Added direct ABI malformed-key regression before correcting storage:22167 ended101
because an opaque table key became missing-property success. Tightened find guards
to UTF-16 or valid0–7 native keys. Focus91720 terminal0: forged keys/stride/owner/name
errors are tagged language exceptions, never traps; native entries stay intact
through GC. CLI guarded30237 terminal0: interfaces5/namedproperties2/indexedstrings2/
mutablefields2/nativeprotocols11. Logs /private/tmp/suss-named-property-malformed-
{red,fix}.log and /private/tmp/suss-named-property-guarded-cli.log.

Two existing source reviews extended, overlay80/985; source artifacts stay30 with
review hash only. Next full ABI/provenance and required workspace baseline, then
independent PR review/fixes and exact final-head CI. No issue or milestone closure.
Continue actual source Object methods/canonical empty list/List/EmptyList/Cons/
IndexedSeq and hashing/reduction/rest/apply toward the full M2–M9 objective.


### Public native property aliases and truthiness

Live GitHub check retains PR85–89 open and unmerged; next PR bases on reviewed89.
Added public native-name probes: fresh98120 certified37 primary observations but
failed natively on property-native.number. Normalized the eight names to existing
native slots;98035 then failed on false marker membership. The pin defines
native-satisfies? under unchecked-if (JS property truthiness), while method macros
explicitly check nil?. A provisional fallback expectation produced a primary
TypeError; corrected probes catch noncallable method errors rather than reporting
that failed expectation as success. Fresh89564 certified47 observations and
retained the false-marker native failure. First helper build8437 failed to compile
an encoder f64 literal; fixed its typed encoding. Fresh69752 certified49 primary
observations and failed only native undefined-method fallback. Native membership
now rejects nil/undefined/false/zero/NaN/empty strings, and method lookup falls back
on nil/undefined alone. Ordinary portable conditional truthiness stays unchanged.

Fresh98331 terminal0:49 exact primary observations and both native tests pass,
/private/tmp/suss-named-property-final49-primary.log. Original22 and intermediate32
expectations remain unchanged. Five named-storage helpers plus one membership
helper; shared prelude/global/version unchanged. Alias fixes extend native protocol
semantics only at verified boundaries. Source inventory80partial/985unassessed and
30 retained source artifacts remain unchanged. Guarded/full validation and
independent PR review/final-head CI are still required before readiness.


Root final guards76636 terminal0: named properties2/native protocols11/interfaces5/
mutable fields2/indexed strings2. Compiler28115 terminal0: ABI19/pipeline17/nominal5.
Python38499 terminal0:71 tests; inventory1065/reviews80+985/core-import30/WIT15/6/
numeric/offline10milestones39issues verified. Required full baseline33237 terminal0,
all enabled workspace/doc tests pass; existing diagnostic differential9 pass/7fail
and legacy/manual ignores remain explicit. Log
/private/tmp/suss-named-property-root-baseline.log. Command:
`CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2 cargo test --workspace --locked -- --test-threads=2`.
All root local Cargo/JVM/Node graphs terminal; mandatory independent review may
own the shared test slot next. No issue closure or merge. Next review/fix and
require final-head CI, then continue retained Object methods and actual sequences.


### Independent PR #90 review

Reviewed candidate38a8a36 against parent09ae837 in isolated
/private/tmp/suss-review-pr90. Three significant findings were fixed:

- Raw instance schemas exposed source names that need munging as wrong property
  values. The pin stores field null as null$: .-null is undefined, .-null$ reads3.
  Initial review corpus35120 ended1 because its provisional combined field
  expectation15 was actuallyNaN. This is not counted as a successful source probe.
  Direct fresh compiled-primary boundary inspection also confirms class prototype,
  function length1 and instance constructor are present; log
  /private/tmp/suss-pr90-review-host-boundaries-primary.log. The positive field
  probe now uses unmunged number/_. Unsupported schema regression64967 ended101
  before the fix. A schema guard rejects reserved/punctuation/non-ASCII names for
  named access while preserving lexical field slots, until munging is adapted.
- A matching valid table prefix hid forged opaque tail keys. ABI regression7924
  ended101 before the fix,40732 ended0 after it. Lookup remembers the first match
  but validates every schema/table key before returning or modifying storage.
  Both stride1 schemas and stride2 tables have matching-prefix negative probes.
- Known unfinished callable/prototype and inherited attributes appeared to be
  absent. Focus12775 ended101 before explicit guards. Callable prototype/name/
  length/caller/arguments/call/apply/bind and inherited Object names now raise
  language errors. Declared unmunged own instance fields still take precedence.
  Literal source identifiers also reject a leading digit with a located error.

Added15 independent primary probes, leaving original49 unchanged. All remaining
native public slots, null/undefined aliasing, native-looking instance fields,
assignment error effects, stored nil and object-marker truthiness execute. New
noncallable probes distinguish default83 from catch89, correcting a coverage gap
where the previous default and catch both returned53. Fresh48191 terminal0:
64 exact primary observations/native4, actual Wasm validation plus forced GC;
/private/tmp/suss-pr90-review-final-primary.log. Intentional constructor replacement
warning retained. Command:
`CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2 sh scripts/test-named-property-oracle.sh`.

CLI79523 terminal0: properties4/nativeprotocols11/interfaces5/mutablefields2/
indexedstrings2. Compiler98381 terminal0: ABI19/pipeline17/nominal5. Logs
/private/tmp/suss-pr90-review-{cli,compiler}-guards.log. Python71 tests and pinned
inventory1065/reviews80+985/core-import30 pass. WIT15files/6packages, numeric
manifest and offline roadmap10milestones/39issues verified by wasi_lock.py,
numeric_runtime.py --check and publish_roadmap.py. Initial guessed verifier command
names and --offline flag did not exist; corrected commands pass. Shared ten types,
globals, ABI version and source forms remain unchanged; manifest review hash only.

Required full workspace baseline32212 terminal0: all enabled workspace/doc tests
pass; diagnostic differential9 pass/7 exact failures and existing manual/legacy
ignores remain explicit. Log /private/tmp/suss-pr90-review-full-baseline.log.
Command: `CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2 cargo test --workspace --locked -- --test-threads=2`.
All reviewer Cargo/JVM/Node graphs are terminal; shared test slot is released.
Prototype/Object methods, host attributes, munged names and complete collections
remain explicit unfinished scope. No issue/milestone closure or merge. Next final
review-head CI, then retained Object methods/canonical empty lists and actual
List/EmptyList/Cons/IndexedSeq, hashing/reduction/rest/apply toward M2–M9.


## Type-method scope correction — 2026-09-30

Previous goal turn made authoritative progress: PR90 reviewed d155059,64 exact
primary/native4 and full baseline passed. Root inspected exact-headCI36801132345
success (/private/tmp/suss-pr90-final-ci.log) and marked PR90 ready without merging.
PR85–90 remain stacked/open at the last checks. Full M2–M9 goal stays active.
Separate Object preparation55f5704 in /private/tmp/suss-core-object-methods is
local,24 fresh primary matches/native unresolved Object; no acceptance claim.

The Object source audit exposed an existing type-method scope discrepancy.
Isolated portable/core-type-method-scope starts from reviewedPR90. Fresh47266
terminal101: all8 primary expectations match, then actual native35 versus55 for
global11/local7. Corrected original HIR analysis to remove enclosing locals and
field aliases for deftype methods only, restoring them after analysis. Runtime
extend-type retains enclosing captures; parameters/own fields/inner functions
keep their proper scope. Pinned source is analyzer.cljc parse-type3614–3649 and
core.cljc deftype1778; no source form copied or ABI/runtime layout changed.

Fresh11681 terminal0 for original8. Added two restoration probes and a located
compile-atomic/unpublished constructor/arrow/recovery regression. Fresh94267
terminal0:10 exact primary matches and native2 after actual validation/GC.
Logs /private/tmp/suss-type-method-scope-{primary-native-red,primary-fix,final-primary}.log.
Command: `CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2 sh scripts/test-type-method-scope-oracle.sh`.
One existing review extended,80 partial/985 unassessed and30 artifacts unchanged.
Next guarded tests/provenance/full baseline, independent PR review/significant
fixes, exact final-head CI. No issue/milestone closure or merge. Then actual retained
Object methods/canonical empty list/concrete sequence types/hash/reduction/rest/apply.


Root guarded78181 terminal0: interfaces5/session33/mutablefields2/properties4/
nativeprotocols11/scopes2. Compiler14454 terminal0: nominal5/pipeline17. Provenance
39598 terminal0: Python71/inventory1065/reviews80+985/artifacts30/WIT15/6/numeric/
offline10milestones39issues. Required full baseline73028 terminal0: all enabled
workspace/doc tests pass, with diagnostic9 differential passes/7 exact failures
and existing legacy/manual ignores still explicit. Log
/private/tmp/suss-type-method-scope-root-baseline.log. Command:
`CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2 cargo test --workspace --locked -- --test-threads=2`.
All root local graphs terminal. Next independent review/fixes and final-head CI;
then restore the retained Object-method implementation path. No issue closure/merge.


## Independent review of PR #91 — 2026-10-01

Review worktree /private/tmp/suss-review-pr91 starts at candidate 061412d,
with parent reviewed PR #90 d155059. The production change follows pinned
analyzer.cljc parse-type: enclosing locals are replaced by the new type fields;
restoring locals/fields before propagating a method-analysis error preserves the
surrounding environment. Runtime extend-type remains on its normal capture path.
No significant production finding remains.

Four independent source probes cover namespace-qualified global access against
an own field, self type and arrow constructor references despite enclosing local
shadows, and sibling-method parameter isolation. Original ten cases and their
expectations are unchanged. Fresh oracle/native session 8599 ended 0: all fourteen
pinned observations match independently decoded validated Wasm after forced GC,
and both native tests pass. Log /private/tmp/suss-pr91-review-primary.log.
The pin emits an undeclared generated-arrow warning for the self-reference probe;
the warning remains visible and its executed value matches. Command:
`CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2 sh scripts/test-type-method-scope-oracle.sh`.

Python 71 tests, inventory 1,065, reviews 80 partial/985 unassessed, imported
artifacts 30, official WIT 15 files/6 packages, numeric manifest and offline
roadmap 10 milestones/39 issues pass. The manifest changes only the review hash.
Source forms, shared runtime ABI and production code are unchanged by review.

Required full workspace baseline session 99149 ended 0: all enabled workspace
and doc tests pass. Existing manual/legacy ignores and the diagnostic differential
baseline of nine passing/seven exact failing observations remain explicit.
Log /private/tmp/suss-pr91-review-full-baseline.log. Command:
`CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2 cargo test --workspace --locked -- --test-threads=2`.
All review Cargo/JVM/Node graphs are terminal; shared test slot is released.
Next require exact reviewed-head CI before readiness, without merging. Then
continue retained Object methods and real persistent sequence/list foundations.
No issue/milestone closure or full-core acceptance claim.


## Root PR90 final gate and Object prerequisite — 2026-09-30

Previous goal turn made authoritative progress: PR90 independently reviewed
fixesd155059, fresh64 primary/native4 and full reviewer baseline passed. Root
inspected exact-head CI36801132345 success, including enabled property4, ABI19,
Python71/reviews80+985/artifacts30 and full workspace. Log
/private/tmp/suss-pr90-final-ci.log. Marked PR90 ready, no merge or issue closure.

Separate portable/core-object-methods worktree now records24 fresh source probes.
Initial96832 ended1 at strict comparison: detached method returned canonicalNaN,
and unbound outer factor produced a primary undeclared-var warning andNaN. Recorded
those failed provisional expectations honestly. Revised known-global11/local7
probe returns55 rather than captured35. Fresh95816 terminal101: all24 exact primary
matches, then native unresolved Runtime name Object at43..49; see
/private/tmp/suss-object-method-primary-native-red.log and docs/runtime/object-methods.md.
No native Object acceptance claim or review/source count change. All root local
graphs terminal. Next inspect existing type-method lexical scope with a separately
executing protocol regression, then adapt Object methods without deleting retained
source method blocks. Canonical empty lists and concrete collections remain open;
full M2–M9 objective remains active.

## Root PR91 final gate and Object continuation — 2026-10-01

Independent review final6e5d9ed passes exact-head CI36808444957. Root downloaded
/private/tmp/suss-pr91-final-ci.log and inspected enabled scope tests, Python71,
80 partial reviews/985 unassessed and the full workspace results. PR91 is ready
for user merge; no merge or issue closure. Retained Object preparation5080a6c
rebased onto this reviewed parent. Focused graph28388 ended101 at unresolved
Runtime name Object43..49; all24 fresh primary observations remain certified.
Next implement Object methods and execute the strict native corpus.

## Object methods implementation candidate — 2026-10-01

Original HIR/runtime adapters preserve pinned Object receiver/recur, named shared
unbound functions, descriptor-owned extension/redefinition and lookup-before-args.
The existing ten shared GC prelude types/version/old globals remain unchanged;
private method tag/default-this realm append globals. Public prototypes, computed
or munged names, extra/default-realm fields and full JS interop remain unsupported.
No dead-owner registry or fabricated persistent collection.

Original24 primary probes now pass native execution (graph35565, terminal0).
Expanded44 fresh primary graph88291 matched all reference values then native failed
at new recur probe's unsupported general =. Replaced that probe with <= without
changing original24. Fresh88197 terminal0: all44 primary/native values exact after
GC; log /private/tmp/suss-object-method-final44-primary-second.log. Pin warnings
remain visible; generated JS confirms recur replaces user args while retaining this.
Native malformed receiver/signature/name/recur/unknown-var forms fail compile
atomically and recover. CLI graph2156 terminal0: Object2/property4/native11/type-scope2/
interfaces5. Compiler graph9692 terminal0: nominal5/pipeline17/ABI19. Logs
/private/tmp/suss-object-method-cli-guards.log and
/private/tmp/suss-object-method-compiler-guards.log.

Fn and fn? retain two additional pinned forms; fn? has an explicit defn-bootstrap
patch preserving marker branch/short circuit/docstring. Original js-fn? primitive
recognizes shared closures. Python71/inventory1065/reviews82 partial+983 unassessed/
core import32 files pass graph54165 terminal0. Source selection28 forms; no full
macro/core/collection acceptance claim. Log /private/tmp/suss-object-method-provenance.log.
All local Cargo/JVM/Node graphs terminal. Next independent PR review/fixes and
required full baseline/final reviewed-head CI; then retained persistent collections.
No issue closure or merge. M2–M9 goal remains active.

## Independent review of PR #93 — 2026-10-01

Review starts at candidate2f1ceb4 in /private/tmp/suss-review-pr93, based on merged
main89e5dce123db7dbeba15868f0b2dd9f0497fabf1 (#90 and #91 are already merged).
Three findings were fixed: the resident-cell regression must count the new js-fn?
primitive (30 rather than29); __proto__ method/field declarations must reject
unfinished prototype mutation rather than fabricate ordinary storage; and a
foreign closure copying the detached callback with nil/wrong-tag environment
must fail through the language exception rather than recursively invoke itself.
Source-field rejection protects physical protocol slots, while the runtime schema
guard protects host-created named storage. Compile errors retain exact spans and
publish no preceding definitions. No other significant production finding remains.

Pinned compiler.cljc emits constructor this.<field> assignments and host method
assignments; a separate fresh primary namespace confirms primitive __proto__
initialization has no own field/does not read7 and an Object __proto__ method
changes the prototype to a function. It also confirms raw null/constructor methods
return13/29 and both raw named reads are functions; emit-dot uses an empty reserved
set for these paths, unlike constructor field munging. These are diagnostic boundary
observations, not additional matching native corpus cases. Log
/private/tmp/suss-pr93-review-prototype-reserved-primary.log, session39940 ended0.

Eight independent source probes preserve the original44 unchanged: duplicate
arity last-wins, shared detached implicit-this identity, nested receiver capture,
properties on method wrappers, noncallable own-field argument order, parallel
recur bindings, parameter shadows, and Object call versus Fn marker. Fresh final
source graph37647 ended0 with52 exact primary/native observations and both native
tests. Pinned duplicate-arity/protocol-recur warnings stay visible. Log
/private/tmp/suss-pr93-review-primary-wrapper-fixed.log. Command:
`CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2 sh scripts/test-object-method-oracle.sh`.
An initial provisional wrapper result was corrected from124 to24 before final
certification; old instances retain original class methods. A discarded js-fn?
value probe produced primary macro-value warnings; the retained Fn probe tests
public fn?. Earlier strict compare session6823 failed; fresh final probes pass.

The foreign copied-callback ABI regression first failed (session2438 terminal101)
with call-stack-exhausted: /private/tmp/suss-pr93-review-detached-wrapper-red.log.
After private environment/tag validation, all20 ABI tests pass (31161 terminal0),
including opaque tail keys after matching prefixes, malformed tagged names,
corrupt payloads, nil/wrong-tag copied callbacks and host-created __proto__ named
get/set. Log /private/tmp/suss-pr93-review-abi-wrapper-fixed.log. Command:
`CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2 cargo test -p suss-compile --test runtime_abi --locked -- --test-threads=2`.
The first ABI fixture used an unavailable StructRef setter (8037 compile101);
final fixtures use supported Args mutation and a validated shared-prelude module.

Python71/inventory1065/reviews82 partial+983 unassessed/import32 files/WIT15 files
and6 packages/numeric manifest/offline roadmap10 milestones+39 issues all pass.
Log /private/tmp/suss-pr93-review-provenance-wrapper-fixed.log. Only review hash
changes in the generated import manifest;28 selected forms/32 licensed artifacts
and full-core/macro/collection limitations remain unchanged.

Initial full baseline14475 failed the stale29-cell assertion. Baselines98735,
41097 and64057 passed their earlier snapshots; later boundary fixes supersede
those runs. Final required workspace baseline results follow below. The exact
reviewed-head CI gate remains root work before readiness; no issue closure or merge.
Next unblocked task is retained collection dependencies and complete persistent
sequence/list foundations, with Object blocks preserved. M2–M9 remains active.

Final required full workspace baseline44285 ended0: all enabled workspace and
doc tests pass, including Object2/property4/native11/type-scope2/interfaces5,
nominal5/pipeline17 and ABI20. Existing manual/legacy ignores and the diagnostic
nine passing/seven exact failing observations remain explicit. Log
/private/tmp/suss-pr93-review-full-baseline-wrapper-fixed.log. Command:
`CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2 cargo test --workspace --locked -- --test-threads=2`.
All review Cargo/JVM/Node graphs are terminal. Shared test slot is released;
root must require exact final reviewed-head CI before readiness. No merge/closure.


## Retained caching-hash preparation — 2026-10-01

Separate portable/core-caching-hash worktree starts on Object candidate2f1ceb4.
Original bounded HIR expansion follows pinned core.cljc1284, preserving one cache
read, nil/undefined test, hit suppression and ordered miss/assignment. Twenty-four
provisional shared scalar probes and compile-atomic negative guards are prepared.
They have NOT run against JVM/Node or native Wasm yet; no compatibility success,
review count, source selection or issue status is changed. The shared test slot
belongs to independent PR93 reviewer; root has run only pure generation/diff checks.
Next rebase onto reviewed PR93, acquire the released test slot, certify fresh
primary expectations and execute the native regressions, then align provenance
and dispatch independent review/final CI if a PR is opened. List/Cons hashing and
complete collections/M2–M9 remain open. See docs/runtime/caching-hash.md.

## Caching-hash executing candidate — 2026-10-01

Rebased preparationeeacae0 onto independently reviewed Object head e958472.
PR93 final-head CI36813244361 is still live; it is not yet ready for merge.
Its source52/native2/ABI20/Python71 and required reviewer baseline passed, including
significant prototype/callback fixes. Root has not merged or closed any issue.

Fresh caching-hash graph46126 ended0:24 exact pinned observations and native2
pass with independently decoded validated Wasm/GC. Log
/private/tmp/suss-caching-hash-primary-candidate.log. Command:
`CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2 sh scripts/test-caching-hash-oracle.sh`.
Cached false/zero/negative-zero/NaN/string return unchanged; hits suppress both
operands, misses evaluate callee then collection and assign exactly one result,
throws preserve nil cache, nil results recompute. Protocol fields/nested captures/
Object methods and qualified globals execute. Negative immutable/local/shadowed/
unknown/non-symbol/wrong-arity keys recover without definition/type publication.

CLI graph18075 terminal0: caching2/control3/mutable2/Object2/scope2. Compiler
92720 terminal0: nominal5/pipeline17. Logs /private/tmp/suss-caching-hash-cli-guards.log
and /private/tmp/suss-caching-hash-compiler-guards.log. Python graph91870 terminal0:
71 tests/inventory1065/reviews83 partial+982 unassessed/import32 verified. One
partial macro review is added; no macro source copied,28 selected forms/32 artifacts
change only their review hash. Log /private/tmp/suss-caching-hash-provenance.log.
All root Cargo/JVM/Node graphs terminal. Next independent PR review/fixes, required
full baseline and exact final-head CI before readiness, then continue complete
persistent List/Cons/IndexedSeq dependencies. Macro bootstrap/full hashing and
M2–M9 acceptance remain open. No issue closure/merge.

## Independent PR #94 hash-cache review — 2026-10-01

Reviewed candidate cd86c58 against independently reviewed Object base e958472.
No significant production defect was found. The bounded HIR expansion mirrors
pinned core.cljc 1284: a fresh binding captures the single cache read; nil/internal
undefined branch to callee-before-collection invocation; assignment uses the
resolved original global or mutable receiver field; other cached values suppress
both operands. Receiver captures and lexical/global resolution remain explicit.
No runtime, ABI, source selection or compiled macro claim changes.

Fourteen independent probes preserve the original 24 source/expectations exactly.
They cover callee throws, operand cache mutation, false and undefined results,
dynamic binding/redefs restoration, callable hit identity, local macro-name calls,
and qualified global keys beneath local and method-field shadows. Initial graph
85454 ended 101: all 38 pinned observations matched, but a new hyphenated Object
field correctly hit the documented unmunged-schema runtime boundary. The revised
probe uses an underscore field; fresh graph 86874 ended 0 with all 38 primary/native
observations and both then-existing native tests passing. Logs:
/private/tmp/suss-pr94-review-primary.log and
/private/tmp/suss-pr94-review-primary-field-fixed.log. Command:
`CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2 sh scripts/test-caching-hash-oracle.sh`.

A third native test executes core aliases/exclusions and checks expansion in both
runtime and macro phases. Malformed/immutable-key guards now assert nonempty
source spans as well as atomic recovery. Focused graph 37593 ended 0: all three
native tests pass. Log /private/tmp/suss-pr94-review-macro-scopes.log. Command:
`CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2 cargo test -p suss-cli --test portable_caching_hash --locked -- --test-threads=2`.

Python graph 96556 ended 0: 71 tests, inventory 1065, overlay 83 partial reviews /
982 unassessed, 32 import artifacts, 15 WIT files / 6 packages, numeric manifest,
and offline 10 milestones / 39 issues all verified. Only the generated import
review hash changes; 28 selections and all license/source bytes stay unchanged.
Log /private/tmp/suss-pr94-review-provenance.log. Next require final workspace
baseline and exact reviewed-head CI; complete List/Cons hashing and compiled
macro bootstrap remain unblocked future work. No issue closure or merge.

Pre-fix full graph 98285 ended 0 on ARM, but candidate Linux CI run 36813807935
failed nan-is-cached: division produced negative canonical NaN instead of positive.
The accepted design already permits that arithmetic sign difference (2026-09-29
clarification); scripts/oracle_compare.py retains raw bits and permits only the
canonical payload's sign. Review now applies this rule solely to nan-is-cached,
preserving all original 24 source/expectations and raw decoded observations.
All other cases remain exact. New guards reject payload differences, infinity,
finite/zero changes, signed-zero differences and NaN sign differences in other
storage cases. A separate native test produces both canonical signs and checks
exact cache read/hit/post-GC bits while throwing operands certify suppression.
No production arithmetic or ABI changes. Initial NaN regression graph 67407
ended 101 due to an inferred closure borrow lifetime; the helper now has explicit
SessionValue lifetimes. Final fresh reference and full-baseline results follow.

Final fresh reference graph 67526 ended 0: all 38 pinned observations and all four
native tests pass, including both canonical NaN signs and exact cache storage.
Log /private/tmp/suss-pr94-review-primary-nan-fixed2.log. Required full workspace
graph 19936 ended 0: every enabled workspace/doc test passed, including cache4,
Object2, scope2, nominal5, pipeline17 and ABI20. Existing manual/legacy ignores
and diagnostic 9 passing / 7 exact failing observations remain explicit. Log
/private/tmp/suss-pr94-review-full-baseline-nan-fixed.log. Command:
`CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2 cargo test --workspace --locked -- --test-threads=2`.
Final provenance graph 93228 ended 0; Python 71 and all manifest checks pass.
Log /private/tmp/suss-pr94-review-provenance-nan-fixed.log. All Cargo/JVM/Node review
graphs are terminal. Next root requires exact reviewed-head CI before readiness;
then continue retained persistent collections and full hashing/macros. No merge
or issue closure. The shared test slot is released after the review push finishes.


## Object final CI gate and bitwise preparation — 2026-10-01

PR93 reviewed final head e958472c3d1514eedda07f4c2966d4a1e4243001 passed
CI36813244361. Root inspected /private/tmp/suss-pr93-final-ci.log: Python71,
82 partial/983 unassessed reviews,32 artifacts, enabled Object2 and ABI20,
and the required full workspace baseline. PR93 is marked ready; no merge.
PR94 reviewed final head74120cb54a9abac458825658632f41fd9eb8d8ad is still
awaiting CI36814856668; its local reviewed full baseline passed. Do not claim
remote success or readiness until that exact run is inspected. Both remain Refs;
complete issue acceptance is not fulfilled and no issue is closed by this slice.

New unpublished worktree /private/tmp/suss-core-bitwise-hash starts at reviewed
PR94. Original37-case corpus covers signed wrapping, shifts, imul, scalar coercion,
computed/variadic calls and order. Fresh pinned primary observations match exactly;
native session29394 ended101 at unresolved int. Aggregated session97954 ended101
with36 unresolved observations and one matching local-shadow case. An unrelated
unsupported defn in the new order probe was replaced with def/fn; fresh final
session44068 ended101 with37 exact primary observations and36 unresolved native
names, now including the intended bit-or failure. Logs:
/private/tmp/suss-bitwise-hash-preparation.log,
/private/tmp/suss-bitwise-hash-native-red.log,
/private/tmp/suss-bitwise-hash-preparation-final-red.log.
Command: `CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2 sh scripts/test-bitwise-hash-oracle.sh`.
No failures are ignored or accepted into the passing baseline. This red test is
local preparation, not a new published PR or implementation claim.

Separate pinned arity graph11590 ended0 and Node ended0, producing24 diagnostic
observations in /private/tmp/suss-bitwise-arity-observations.json. Captured wrappers
may fill undefined/ignore extra args; direct macros are a separate surface.
These diagnostic observations are not additional native matches. See
 docs/runtime/bitwise-hashing.md. No source forms copied and no review count,
source-selection, issue or milestone status changes. All local Cargo/JVM/Node
processes are terminal. Next implement bounded original bitwise coercion and
certify advertised arities with retained-source provenance before a new PR.
Complete List/Cons/IndexedSeq and M2–M9 remain open; no merge or issue closure.


## Private 32-bit coercion prerequisite — 2026-10-01

A focused runtime regression first hit a missing RootScope import (21273 compile101);
corrected fixture47448 ended101 at missing coerce-int32, the intended red stage.
Original runtime helper now calls existing coerce-number, truncates once, guards
nonfinite values before conversion and wraps modulo2^32 using exact binary scaling.
No public form, GC layout, language cell, source selection or ABI version changes.
Focused48691 ended0. Extended fixture initially called gc on RootScope (compile101);
forced-GC cases now use Store's rooted default scope. Final83289 ended0 with all21
ABI tests, including20 numeric boundaries/2048 varied encodings, nil/Boolean/string
coercion, UTF-16 after GC and unsupported object language exception with recovery.
Logs /private/tmp/suss-int32-abi-red.log, /private/tmp/suss-int32-abi-red2.log,
/private/tmp/suss-int32-abi-candidate.log, /private/tmp/suss-int32-abi-all-candidate.log,
/private/tmp/suss-int32-abi-all-candidate2.log.
Command: `CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2 cargo test -p suss-compile --test runtime_abi --locked -- --test-threads=2`.
Public37-case native corpus remains red36 unresolved/one match. No full-baseline or
public compatibility claim and no new PR opened. Next integrate bounded bitwise
operations and retained public source, then repeat primary/native/provenance/full
baseline, independent PR review and exact final-head CI before readiness. All
local test processes are terminal; PR94 final remote CI remains pending.


## Hash-cache final CI gate — 2026-10-01

PR94 final reviewed head74120cb54a9abac458825658632f41fd9eb8d8ad passed
CI36814856668, completed04:43:13Z. Root downloaded and inspected
/private/tmp/suss-pr94-final-ci.log: Python71,1065 declarations,83 partial/982
unassessed reviews,32 licensed artifacts, enabled caching4 (including exact NaN
storage/GC and comparison guards), Object2, ABI20 and the required full workspace
baseline/doc tests pass. PR94 body records independent review fixes and the exact
CI link; it is marked ready. PR93 is also ready with its verified final-head CI.
Root did not merge any PR or close any issue. Refs links remain partial.

Private coercion follow-up guards17446 ended0 with pipeline17; source guard69649
ended0 with caching4/Object2. Logs /private/tmp/suss-int32-pipeline-guards.log and
/private/tmp/suss-int32-source-guards.log. Commands use the shared target/two build
workers and --locked -- --test-threads=2; source guards select -p suss-cli
--test portable_caching_hash --test portable_object_methods, compiler guards
select -p suss-compile --test portable_pipeline. No broader public/full-baseline
claim is added for the unpublished bitwise branch; its37-case native corpus
remains red36 unresolved/one match. All local processes are terminal. Next
integrate bounded bitwise/imul and retained-source provenance, then review and
require final-head CI on any new PR. Complete persistent collections/M2–M9 open.


## Bitwise hashing dependency executing candidate — 2026-10-01

Original checked runtime helpers now provide int/bitwise/imul function values in
canonical live cells, with bounded HIR/IR macro expansion for the pinned direct
forms. Native bootstrap cells45 replace30; GC layouts/ABI version unchanged.
Direct variadic folds preserve pair conversion before later syntax; computed
calls evaluate every argument before folding. Off-arity captured JS wrappers
remain explicit differences under the accepted diagnostic contract, not matches.
Retained bit-count3057 and int-rotate-left947 preserve their original algorithms
(including bit-count arithmetic), metadata/docstrings with explicit fixed defn
patches/EPL artifacts.30 selected forms/34 files;113 partial reviews/952 unassessed.
Conditional imul inside if is not emitted by the top-level scanner; explicit
source-region/file hashes record provenance without fabricated declaration IDs.

Initial73096 compile101 exposed an argument-pattern binding typo in capture
traversal; fixed73868 ended101 with35 matches/two unresolved bit-count cases.
The first artifact command incorrectly used unsupported --write; no generated
source was updated and72714 stayed red on the two missing functions. Default
core_import.py then regenerated34 files; fresh71588 ended0 with all original37
primary/native observations. Expanded final63160 ended0 with54 exact observations,
original37 unchanged, including retained algorithms/namespace/redefs/capture.
Log /private/tmp/suss-bitwise-public-final54.log. Command:
`CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2 sh scripts/test-bitwise-hash-oracle.sh`.

Source guard55691 ended0: bitwise4 and persistent-session33, including located
atomic arity errors, aliases/exclusions/both phases and unsupported conversion
order with recovery. Compiler57537 ended0: public HIR/IR bitwise guard1,
pipeline17/ABI21. Logs /private/tmp/suss-bitwise-source-guards.log and
/private/tmp/suss-bitwise-compiler-guards.log. Commands select corresponding
-p suss-cli --test portable_bitwise_hash --test persistent_session or
-p suss-compile --test portable_bitwise --test runtime_abi --test portable_pipeline,
using shared target/two build workers and --locked -- --test-threads=2.
Python71, inventory1065/reviews113+952/import34/WIT15 files+6 packages/numeric
manifest/offline roadmap10+39 pass. Log /private/tmp/suss-bitwise-python.log.
An incorrect numeric_artifact.py command failed because that file does not exist;
correct scripts/numeric_runtime.py --check passed. A dependency-overlay helper
first matched a referenced ID instead of an entry; failed before writing and the
corrected helper/generated manifest validated. No failures hidden or ignored.
All local Cargo/JVM/Node graphs are terminal. Next independent PR review/fixes,
required full workspace baseline and exact reviewed-head CI before readiness.
No issue closure/merge; full List/Cons/Murmur hashing/macros/M2–M9 remain open.


## Independent PR95 review — 2026-10-01

Review worktree /private/tmp/suss-review-pr95 starts at candidate458b5df on
portable/core-bitwise-hash, base reviewed PR94 head74120cb. Original54 source
cases/expectations are unchanged; sixteen independent probes add extreme finite
and subnormal conversion, low bits near2^53, negative/fraction/infinite shift
counts, wrapping multiplication, scalar coercion, retained hashing algorithms and
nested captured callbacks. Fresh first graph98551 ended0:70 exact primary/native
observations and native4. Log /private/tmp/suss-pr95-review-primary70.log.
The pre-fix required full baseline40057 ended0, but is superseded by the significant
captured variadic-body fix below; do not treat it as the final review gate.

Significant semantic finding: variadic callbacks used an immutable primitive for
every tail operand, whereas the retained core definition passes its current var
once to reduce after calculating the original primitive initial pair. Native red
2922 ended101 with7 versus107. The original function body is now preserved while
its tail reducer resolves through its canonical live cell, including dynamic
frames; earlier reducer calls redefining the var do not replace the reducer already
passed to reduce. Variadic initializer exports now accept their canonical self cell;
the native session allocates that cell before constructing its closure and then
publishes the resulting value into the same cell. Cell count45/shared GC layouts/
ABI version unchanged. A checked callback environment rejects malformed nil/opaque
cells as language exceptions rather than Wasm casts/traps.

Fresh pinned public calls through saved generic JS wrappers throw four exact
TypeErrors on the replacement global variadic property. They remain explicit
observations outside the70 equal public cases. Five development-only js* probes
independently inspect the retained implementation bodies:107/104/107/105 for the
four current reducers and215 when the first tail call redefines its own var.
The var is read once, not per iteration. These source adaptations are recorded in
 tests/oracle/bitwise-capture-observations.json and certified separately by
 scripts/bitwise_capture_oracle.py; host interop/public-wrapper equality is not
advertised. The initial diagnostic fixture54395 ended1 at an unmatched delimiter;
corrected21810 ended0. Native first fix8097 ended0 with bitwise5. Logs:
/private/tmp/suss-pr95-review-capture-primary.log,
/private/tmp/suss-pr95-review-capture-primary-fixed.log,
/private/tmp/suss-pr95-review-capture-native-red.log,
/private/tmp/suss-pr95-review-capture-native-fixed.log.

Provenance finding: the candidate imul region953–966 included a following unrelated
Murmur comment. Corrected953–964 hashes precisely the complete conditional form.
New scripts/bitwise_provenance.py verifies the pin, file hash, region hash, complete
form bounds and both conditional definitions; explicit CI and mutation gates reject
stale hashes, truncated/extra source, invalid ranges and duplicate metadata. The
candidate JSON is rejected by the new exact-form validator. No declaration IDs are
invented for definitions absent from the top-level scanner. Retained source/license
selection remains30 forms/34 artifacts,113 partial reviews/952 unassessed.

Final focused source graph65015 ended0: bitwise6/persistent-session33, including
GC, dynamic scope, noncallable/wrong-arity/throwing reducers and recovery. Compiler
fixture48942 failed to compile on an I31 Option; corrected17971 ended101 because
the fixture requested nonexistent args-set. Typed Wasmtime array writes now exercise
the actual shared ABI. Final27147 ended0: bitwise1/pipeline17/ABI22 including all
malformed-cell guards after GC. Logs /private/tmp/suss-pr95-review-source-final.log,
/private/tmp/suss-pr95-review-compiler-final.log,
/private/tmp/suss-pr95-review-compiler-final-fixed.log,
/private/tmp/suss-pr95-review-compiler-final-fixed2.log. Commands select respective
-p suss-cli --test portable_bitwise_hash --test persistent_session or
-p suss-compile --test portable_bitwise --test portable_pipeline --test runtime_abi,
with shared target/two workers, --locked -- --test-threads=2. An incidental workspace
format command touched unrelated formatting; all34 unrelated files were restored
before final checks. No unrelated change is included.

Final fresh graph90990 ended0: all70 exact primary/native public observations,
4 exact pinned wrapper errors,5 separate internal-body diagnostics and native6.
Log /private/tmp/suss-pr95-review-primary-final70.log. Command:
`CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2 sh scripts/test-bitwise-hash-oracle.sh`.
Final provenance graph89710 ended0: Python76, inventory1065/reviews113+952/import34,
conditional imul exact-region gate, WIT15 files/6 packages, numeric manifest and
offline roadmap10 milestones/39 issues pass. Log
/private/tmp/suss-pr95-review-provenance-final.log. Negative diagnostic-observation
regressions reject malformed schema, missing/duplicate results and changed errors
or bits. No failure, ignored test or divergence is relabeled as a success.

Final required full workspace graph12800 ended0: every enabled workspace/doc test
passes, including bitwise6/persistent-session33, compiler bitwise1/pipeline17/ABI22
and the existing strict legacy/diagnostic suites. Manual/legacy ignored tests and
9 passing/7 exact failing diagnostic observations remain explicit. Log:
/private/tmp/suss-pr95-review-full-baseline-final.log. Command:
`CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2 cargo test --workspace --locked -- --test-threads=2`.
Candidate CI36817667394 succeeded at458b5df and does not certify these review fixes.
All local Cargo/JVM/Node processes are terminal; the shared slot is released once
the review push finishes. Root requires exact reviewed-head CI before PR95 readiness. No merge,
issue closure or milestone acceptance claim. Next retain actual Murmur/ordered
hashing and persistent sequence/list source; complete core/macros/M2–M9 remain open.
## Scalar Murmur hashing preparation — 2026-10-01

New separate branch portable/core-murmur-hash/worktree /private/tmp/suss-core-murmur-hash
starts at PR95 candidate458b5df.35 provisional scalar probes and native test runner
are prepared; original bounded ->/as->/zero? expansion is drafted but NOT compiled
or tested. No retained forms or review statuses are added. PR95 reviewer exclusively
owns Cargo/JVM/Node slot, with review-expanded70 primary/native observations passing
and full baseline40057 live. Reviewer found/corrected imul provenance bounds and is
adding exact source-region/file/pin verification to CI with Python75 passing.
Next rebase onto final reviewed PR95, then acquire the released slot and first
certify fresh pinned observations/native red, retain the scalar Murmur forms with
EPL/patch provenance, execute focused/full checks and dispatch a reviewer for any
new PR. PR95 final reviewed-head CI is still required before readiness. Do not
claim List/Cons/hash-ordered/hash-unordered/string/numeric hashing complete or
substitute private argument buffers for persistent source collections. See
 docs/runtime/murmur-hashing.md. No PR merge or issue closure.


The unpublished Murmur preparation now has52 provisional probes (original35
unchanged).17 new ->/as->/zero? cases cover lexical shadowing, captures, tail recur
and strict zero testing. They have not run against the pin/native and are NOT
success evidence. PR95 reviewer is investigating captured variadic tail dispatch;
root still holds no local test slot and has launched no Cargo/JVM/Node processes.

One additional unexecuted thread-callee ordering probe makes53 provisional cases.
Nested threading syntax must preserve outer callee evaluation before the initial
expression, rather than pre-evaluating that expression as an eager temporary.
No primary/native success is claimed; shared test slot remains with PR95 reviewer.


## Retained scalar Murmur executing candidate — 2026-10-01

Rebased the preparation onto independently reviewed PR95 d49c5a7; one append-only
handoff conflict preserved all reviewer and preparation evidence. PR95 reviewed
full12800 passed and exact CI36819490858 is live; no ready/merge claim for that PR.
Root acquired the released shared test slot. Initial fresh75181 ended101:53 exact
pinned observations,36 unresolved native forms and17 executing control observations.
Log /private/tmp/suss-murmur-hash-first-red.log. Seven explicit fixed defn patches
and three exact constants now retain the scalar Murmur algorithms/zero? with EPL
provenance. Import31363 ended0:53 exact primary/native and native1. Final fresh43112
ended0:57 exact primary/native (original53 unchanged), all four native tests pass.
Log /private/tmp/suss-murmur-hash-final57.log. Command:
`CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2 sh scripts/test-murmur-hash-oracle.sh`.
Pinned non-number zero? numeric warnings stay visible, not blanket-suppressed.

Original bounded ->/as->/zero? expansions preserve splicing/evaluation order,
sequential lexical identities, captures, metadata/spans, statement/tail contexts
and strict numeric zero. General binder destructuring/callable collections and
compiled macro bootstrap remain incomplete. Source guard65413 ended0: Murmur4,
bitwise6/cache4; log /private/tmp/suss-murmur-source-guards.log. Commands select
-p suss-cli --test portable_murmur_hash --test portable_bitwise_hash
--test portable_caching_hash with shared target/two workers/--locked -- --test-threads=2.
Python76 and source/review/import/bitwise provenance checks pass:126 partial/939
unassessed,40 selections/44 licensed artifacts. Log /private/tmp/suss-murmur-python.log.
No native bootstrap cell, shared GC layout or runtime ABI change in this slice.
Compiler focused62882 is still live; final results follow. Required independent
PR review/fixes/full baseline and exact reviewed-head CI remain before readiness.
No closure/merge; hash-ordered/hash-unordered/string/numeric hashing, persistent
List/Cons/IndexedSeq and M2–M9 remain unfinished.


Compiler62882 ended0: bitwise guard1 and pipeline17 pass, log
/private/tmp/suss-murmur-compiler-guards.log. A focused threading metadata
regression61654 ended101: symbol metadata was incorrectly promoted to the generated
call list. Pin core.cljc104 preserves list-step metadata only; the symbol itself
still retains its metadata. Corrected6812 ended0 with the public HIR/validated
artifact regression. Logs /private/tmp/suss-murmur-thread-metadata-red.log and
/private/tmp/suss-murmur-thread-metadata-fixed.log. Command selects -p suss-compile
--test portable_murmur with the required shared target/build workers/locked/two
workers. All root local processes are terminal. Inventory126+939/40 selections44
artifacts, WIT/numeric/offline roadmap and strict bitwise provenance gates pass.
Next independent PR review/fixes, required full baseline and exact final-head CI.
No readiness, milestone acceptance, issue closure or merge claim for this slice.

## Independent PR96 review — 2026-10-01

Review worktree /private/tmp/suss-review-pr96 starts at 74dac8a7114c72504b2acf03960fb326c3565491,
base reviewed PR95 d49c5a72c2661712ddd539414025764ae0c06363. Original 57 source
cases/expectations remain unchanged. Sixteen independent cases bring the fresh
corpus to 73: special-form syntax splicing, nested outer/inner callee effects,
initial-expression shadowing, three generations of captured bindings, nested
as->, statement def, throwing operands, qualified macros, subnormal/Boolean/large
coercion and Undefined/first-class string zero?. First 22099 ended1 on the reviewer's
hand-calculated expected hash-long value; fresh pinned bits were -2147483648,
not 2147483646. Only that new expectation changed. Fresh corrected 25391 ended0:
73 exact pinned/native observations and native 4. Logs
/private/tmp/suss-pr96-review-primary73.log and
/private/tmp/suss-pr96-review-primary73-fixed.log. Command:
`CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2 sh scripts/test-murmur-hash-oracle.sh`.

Significant compiler finding: -> accepted 255 steps under its 256-argument limit
but aborted with a Rust stack overflow. Initial limit regression 71518 ended101
because its isolated session lacked retained inc; changed only the new limit probe
to bootstrap +. Corrected 34083 reproduced SIGABRT, and trial per-chain 63-step
cap 94790 also aborted. A per-chain cap also fails to bound nested macros, so that
trial was removed. Analyzer now checks total recursive analysis depth 24 and emits
a located diagnostic before nesting exhausts the stack. This explicit bootstrap
limit applies to generated and original syntax; stack-safe general analysis remains
future work. No runtime layout, ABI, cell, source selection or algorithm changes.
Final 22465 ended0: native 5, including long single/nested chains rejected atomically,
recovery, executing 16-step -> and 254-step sequential as->. Logs
/private/tmp/suss-pr96-review-source-limits.log,
/private/tmp/suss-pr96-review-source-limits-fixed.log,
/private/tmp/suss-pr96-review-source-limits-checked.log,
/private/tmp/suss-pr96-review-source-depth-guard.log. Focused command selects
-p suss-cli --test portable_murmur_hash with shared target/two workers/locked
and --test-threads=2. Failures are preserved, not relabeled as passes.

Provenance 59155 ended0: Python 76, 1065 declarations, 126 partial/939 unassessed,
40 selected forms/44 licensed files, exact conditional imul region, WIT 15 files/6
packages, numeric artifact manifest and offline 10 milestones/39 issues. Log
/private/tmp/suss-pr96-review-provenance.log. No merge/closure/milestone claim.

PR95 independently reviewed final head d49c5a72c2661712ddd539414025764ae0c06363
passed exact CI36819490858 and root marked that prerequisite ready. No merge.

Required full workspace graph 68936 ended 0: all enabled workspace and doc tests
pass, including Murmur 5, bitwise 6, caching 4, compiler metadata 1/pipeline 17/ABI 22.
Manual ignored tests and the 9 passing/7 exact failing diagnostic observations
remain explicit. Log /private/tmp/suss-pr96-review-full-baseline.log. Command:
`CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2 cargo test --workspace --locked -- --test-threads=2`.
Follow-up final fresh graph 12923 ended 0: all 73 exact primary/native observations
and native 5 pass, including an added ordinary 40-level source diagnostic with
nonempty span, compile-atomic unpublished globals and recovery. The guard restores
depth on every returned Result. Log /private/tmp/suss-pr96-review-primary-final73.log.
Original 57 source/expected entries were checked byte-for-byte against the candidate.
No blanket skips or production failures hidden. All local Cargo/JVM/Node processes
are terminal. Root requires exact reviewed-head CI before PR96 readiness; no merge,
issue closure or milestone acceptance. Next stack-safe general analysis, retained
ordered hashing and real persistent sequence/list source; M2–M9 remain open.

## Retained UTF-16 string hashing candidate — 2026-10-01

Separate branch portable/core-string-hash/worktree /private/tmp/suss-core-string-hash
starts at independently reviewed PR96 3d745f6. Exact CI36821371081 is running; PR96
remains draft. PR95 is ready at reviewed d49c5a7 after CI36819490858 success. The
hashing discussion did not change the accepted pinned ClojureScript contract.

Initial reference fixture30083 ended1 because the pinned analyzer rejected a
literal-nil dot receiver; changed that unexecuted probe to a local nil receiver.
Fresh12342 ended101: 71 exact pinned observations, 66 native failures/five matches
before implementation. Log /private/tmp/suss-string-hash-first-red.log. Three fixed
defn patches retain pos?, m3-hash-unencoded-chars and hash-string* with original
algorithms/metadata/docstrings and EPL packaging. Bounded original positive macro
and native UTF-16 charCodeAt member lookup supply dependencies. Strict provenance
requires a real pinned upstream checkout; an initial escaping symlink was rejected
and replaced with a local shared clone, not a weakened verifier.

Import89431/62673 ended101 from a misspelled runtime function index (number-new),
corrected to existing number-box. Fixed62504 ended0: all original71 native matches.
Final79 fixture76187 ended101 from incorrect new test API calls, corrected3638
ended101 only on a new missing-index expectation. Diagnostic44470 confirmed native
member invocation already fills Undefined and ignores evaluated surplus arguments;
these are pinned interop semantics, not a core function wrong-arity exception.
Added exact primary probes rather than claiming an error or skipping them.
Final fresh32275 ended0: 82 exact primary/native observations (original71 unchanged)
and all three native source guards pass. Logs /private/tmp/suss-string-hash-final82.log
and /private/tmp/suss-string-member-arity-red.log. Command:
`CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2 sh scripts/test-string-hash-oracle.sh`.

Builtin lookup occurs before index evaluation, the shared unbound wrapper retains
no owner, direct calls anchor this and detached ordinary calls throw language
errors. Custom Object charCodeAt methods retain dispatch. Indices use scalar checked
ToIntegerOrInfinity, bounds before integer conversion, UTF-16 units/NaN results.
General JS ToString/prototype mutation/object-index coercion remain explicit open
work. No runtime ABI version, shared GC layout or native core cell changes.
Python76 passes (/private/tmp/suss-string-hash-python.log); inventory1065,
reviews130+935, import43 selections47 artifacts, strict bitwise source, WIT15/6,
numeric source/artifact and offline roadmap10/39 gates pass. Compiler47731 is live
for pipeline/Murmur/ABI guards. Independent PR review/fixes, full baseline and exact
final reviewed-head CI remain required; no ready/merge/closure claim for this slice.
Next complete cached string/numeric/public hashing and actual persistent sequences
for ordered/unordered collection hashing; M2–M9 remain incomplete.

Compiler47731 ended0: pipeline17, Murmur metadata guard1 and ABI23 (new exact
UTF-16 unit/bounds/null/unsupported index/typed recovery guard) pass. Log
/private/tmp/suss-string-hash-compiler.log. Source38883 ended0: String3/Murmur5/
Object2 pass at final candidate source, log /private/tmp/suss-string-hash-source-guards.log.
Commands select those test binaries with shared target/build workers2/locked/
--test-threads=2. All root Cargo/JVM/Node processes are terminal. Shared test slot
will pass exclusively to the independent reviewer after draft publication.

## Independent PR97 review — 2026-10-01

Review worktree /private/tmp/suss-review-pr97 starts at candidate30a2ab7 on
portable/core-string-hash, base independently reviewed PR96 3d745f6. A real local
shared clone of the pinned upstream checkout supports strict provenance verification;
no escaping symlink or verifier weakening. Original82 source/expected entries were
checked byte-for-byte unchanged. Fourteen independent probes bring the fresh corpus
to96, covering negative subnormals, binary/octal/fractional scalar index strings,
large nonwrapping indices, receiver lookup before surplus argument effects, detached
argument effects before failure, custom Object dispatch, live retained imul/mixing
calls, lexical pos? captures, throwing surplus arguments and longer UTF-16 hashing.

First fresh graph22152 ended1 only on the reviewer's provisional long-string hash
expectation; pinned bits41bb6a7dad000000 are now recorded for that new case. Corrected
45596 ended101:96 exact pinned observations but one native compile failure on the
new consistency probe's public `=` call. `=` is an unimplemented public-core operation,
not compatibility success; that limitation remains explicit. The intended numeric
hash consistency probe now uses supported pinned `==`, changing only that new source.
Final fresh17156 ended0:all96 exact pinned/native observations and nativeString3 pass.
Logs /private/tmp/suss-pr97-review-primary96.log,
/private/tmp/suss-pr97-review-primary96-fixed.log,
/private/tmp/suss-pr97-review-primary96-final.log. Command:
`CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2 sh scripts/test-string-hash-oracle.sh`.
No original expectation changed, production failure hidden or blanket skip introduced.

New independent ABI guard89143 ended0. An independently generated module copies
actual anchored/detached typed callbacks into foreign closures with nil/opaque/null
environments. Detached invocation remains a language error; the stateless anchored
body safely ignores its environment when physical receiver/index arguments are valid
and rejects invalid argument buffers without traps. Corrupt tagged payload rejection,
GC and typed recovery are exercised. No significant production defect was found.
Log /private/tmp/suss-pr97-review-callbacks.log. Command selects -p suss-compile
--test runtime_abi runtime_abi_string_method with shared target/build workers2,
--locked -- --test-threads=2. Shared runtime layouts/ABI/core cell counts unchanged.

Provenance48205 ended0:Python76,inventory1065/reviews130+935/import47 licensed files,
strict conditional imul source region, WIT15 files/6 packages, numeric manifest and
offline roadmap10 milestones/39 issues pass. Log
/private/tmp/suss-pr97-review-provenance.log. Required full workspace11226 is live;
terminal results will follow. Exact final reviewed-head CI remains required before
readiness. No merge, issue closure or milestone acceptance claim. Full cached string,
numeric/public/collection hashing, general JS coercion/prototypes, actual persistent
sequences and M2–M9 remain unfinished.

Root inspected prerequisite PR96 exact reviewed3d745f6 CI36821371081 SUCCESS
and marked PR96 ready. No merge. Deferred issue #98 tracks evaluation of alternative
hashing algorithms after M4/M6/M7/M8 prerequisites; the accepted pinned algorithm
contract is unchanged. Root numeric-hash preparation is isolated and launches no
Cargo/JVM/Node while this review owns the shared target/cache slot.

Required full workspace11226 ended0:every enabled workspace/doc test passes,
including String3/Murmur5/Object2, compiler pipeline17/Murmur1/ABI24 with the
independent copied-callback guard. Manual ignored tests and diagnostic9 passing/
7 exact failing observations remain explicit. Log
/private/tmp/suss-pr97-review-full-baseline.log. Command:
`CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2 cargo test --workspace --locked -- --test-threads=2`.
All local Cargo/JVM/Node processes are terminal. Shared slot is released after the
review push finishes; root must require exact final reviewed-head CI before PR97
readiness. No merge, closure or milestone acceptance. Next retained scalar numeric
hashing, cached/public string hashing and actual persistent sequence/list algorithms.

## Numeric hashing preparation — 2026-10-01

PR96 is ready at reviewed3d745f6 after exact CI36821371081 success; root inspected
/private/tmp/suss-pr96-final-ci.log including provenance/Python76/Murmur5/ABI22/full
baseline. No merge. PR97 review has96 exact fresh primary/native observations,
original82 unchanged, with added copied-callback/corrupt-environment ABI coverage;
full11226 is confirmed live under that reviewer's exclusive shared test slot.

User-requested deferred algorithm evaluation is filed as issue#98, including the
Murmur3 correction, XXH3/Rapidhash/keyed-hash alternatives, public/internal/digest
separation, compatibility and collision constraints, actual Wasmtime/browser
benchmark criteria and M4/M6/M7/M8 prerequisite gates. It does not alter current
hashing semantics or the existing milestone exit criteria.

Separate unpublished portable/core-numeric-hash worktree starts at PR97 candidate
30a2ab7.42 provisional numeric hash-double/hash-combine observations and reference/
native runners are prepared but UNEXECUTED. No source selections, review statuses
or production support are added. Expected hash-double values independently model
little-endian Float64Array storage followed by default big-endian DataView words
at offsets0/4 and pinned hash-long XOR; fresh pinned observations must verify these
assumptions before a portable adaptation. The source algorithm's scalar ToNumber
conversion must occur once, preserve binary64 signed zero/NaN handling and retain
live hash-long dependency calls. Do not infer hash-double support from a simpler
XOR of the un-swapped binary64 halves or substitute public hash semantics.

Next rebase this preparation onto final reviewed PR97, acquire its released local
test slot, certify primary/native red, then adapt typed byte access with explicit
source/provenance and execute the artifact. General typed-buffer/host interop and
public numeric/hash/cached string/collection hashing remain incomplete. Do not
claim success from these unexecuted preparations or close M2–M9 issues.

## Retained scalar numeric hashing candidate — 2026-10-01

Rebased numeric preparation onto independently reviewed PR97 8947aaa; preserved
both review and preparation handoff entries through an append-only conflict. PR97
full11226 passed and final CI36823514952 is live; it remains draft. Root acquired
its explicitly released shared Cargo/JVM/Node slot. First31865 ended1 from a copied
reference-runner typo (JSON.numericify), corrected to JSON.stringify. Fresh48310
ended101:42 exact primary matches, all42 native unresolved before implementation.
Logs /private/tmp/suss-numeric-hash-first-red.log and
/private/tmp/suss-numeric-hash-first-red-fixed.log. Native import24198 ended0:42
exact primary-derived native matches.

Two explicit fixed defn patches retain hash-double/hash-combine with original
composition, metadata and EPL provenance. Nonescaping typed storage in hash-double
is adapted to one checked scalar binary64 temporary plus exact big-endian words
at offsets0/4 over specified little-endian bytes. Original live hash-long call
remains after both reads. No general JS typed buffers/object coercion or public
numeric hash acceptance is claimed. Three original private unary compiler/runtime
adapters reuse checked bitwise HIR/IR result/arity verification; they are not public
bitwise macro names or core cells. GC layouts/ABI version/cell count unchanged.

Added10 probes preserve original42. Fresh91061 ended101:52 exact pinned/native
value observations but a new guard incorrectly expected compile-time rejection
for dynamically loaded core function wrong arity. Those calls instead use checked
universal runtime arity, so the guard now verifies language error/recovery;
compile-atomic assertions remain on statically known private primitives. Source4904
ended0: numeric3/String3/bitwise6 pass. Logs
/private/tmp/suss-numeric-hash-final52.log and
/private/tmp/suss-numeric-hash-source-guards.log. Compiler69823 ended0: HIR/IR2,
ABI25, including267 exact binary64 encodings, raw signed/signaling/quiet NaN payloads,
byte order and malformed storage/typed recovery. Log /private/tmp/suss-numeric-hash-abi.log.
Python76/inventory1065/reviews132+933/import45 selections49 licensed files,
WIT15/6/numeric/bitwise/offline roadmap10/39 pass; log
/private/tmp/suss-numeric-hash-python.log. Final fresh corrected fixture is running
and terminal results follow. Commands use shared target/build workers2/locked/
--test-threads=2, with sh scripts/test-numeric-hash-oracle.sh for fresh pin comparison.
Independent PR review/fixes, full required baseline and exact final-head CI remain
before readiness. No merge, closure or milestone acceptance. Cached/public numeric/
string/collection hashing, persistent source sequences and M2–M9 remain unfinished.

Final fresh52520 ended0:52 exact pinned/native observations and numeric3 guards
pass. Log /private/tmp/suss-numeric-hash-final52-fixed.log. All root local
Cargo/JVM/Node processes are terminal; the shared slot will pass exclusively to
the independent reviewer after draft publication. Final reviewed-head CI remains
required; no readiness or merge claim for this numeric candidate.


## Independent PR99 review — 2026-10-01

Review worktree /private/tmp/suss-review-pr99 starts at numeric candidate90f73d5
on portable/core-numeric-hash, base independently reviewed PR97 8947aaa. A real
local shared clone of pinned upstream supports strict provenance; no escaping
symlink or verifier weakening. Original52 source/expected observations were checked
unchanged. Twenty independent probes bring the fresh corpus to72: binary/octal/
Unicode whitespace and negative-zero strings, binary64 normal/subnormal boundaries,
separate high/low word reads, argument mutation of live hash-long, dependency throws,
immutable shift expansion, argument throw ordering and mixed nil/Boolean/string
hash-combine operands. No significant production defect was found.

Fresh49556 ended0:68 exact pinned/native observations plus native numeric3.
Final fresh14165 ended0:all72 exact pinned/native observations and numeric3.
Logs /private/tmp/suss-pr99-review-primary68.log and
/private/tmp/suss-pr99-review-primary72.log. Command:
`CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2 sh scripts/test-numeric-hash-oracle.sh`.
New independent ABI guard23405 ended0, rejecting foreign arrays, invalid i31
sentinels and null references in all three adapters as language exceptions rather
than traps, with recovery after forced GC. Existing267 raw binary64 encodings,
including signed zero and signaling/quiet NaN payloads, remain unchanged.
Log /private/tmp/suss-pr99-review-adapters.log. Command selects -p suss-compile
--test runtime_abi runtime_abi_binary64 with shared target/build workers2/locked/
--test-threads=2. Runtime layouts, ABI version and core cell count unchanged.

Provenance74900 ended0:Python76,inventory1065/reviews132+933/import49 licensed
files, strict conditional imul source region, WIT15/6,numeric manifest and offline
roadmap10/39 pass. Log /private/tmp/suss-pr99-review-provenance.log. Full required
workspace baseline is running; terminal results will follow. Exact final reviewed-
head CI remains required. No merge, closure or milestone acceptance. Full public
numeric/string/collection hashing, arbitrary object coercion, actual persistent
sequences and compiled macro bootstrap remain incomplete.


Required full workspace93986 ended0: every enabled workspace/doc test passes,
including numeric3/String3/Murmur5/bitwise6 and compiler pipeline17/HIR-IR2/ABI26.
Manual ignored tests and diagnostic9 passing/7 exact failing observations remain
explicit. Log /private/tmp/suss-pr99-review-full-baseline.log. Command:
`CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2 cargo test --workspace --locked -- --test-threads=2`.
All local Cargo/JVM/Node processes are terminal. Shared test slot is released after
the review push finishes; root must require exact final reviewed-head CI before
PR99 readiness. No merge, closure or milestone acceptance. Next retained cached/
public string hashing and actual persistent sequence/list algorithms, while full
numeric/public/collection hashing and M2–M9 acceptance remain incomplete.

## Cached string hashing preparation — 2026-10-01

Separate unpublished portable/core-string-hash-cache worktree starts at PR99
candidate90f73d5.48 provisional cache value/effect probes and primary/native runners
are prepared but UNEXECUTED. They cover UTF-16 keys, nil/Undefined bypass, cache
hits suppressing current hasher effects, non-number/zero/NaN cache values,
throwing misses, threshold1024 versus1025, reset-before-nil behavior, replacement
identity, captures/live dependencies and prototype-named keys. Expectations must
be freshly certified; they are not success evidence. PR99 reviewer exclusively
owns the local Cargo/JVM/Node slot, with72 fresh exact observations (original52
unchanged), independent foreign/malformed-value ABI coverage and full93986
confirmed live. No root heavy processes started.

The pinned source creates a plain JS object for this internal cache, calls
unchecked-get/gobject-set and increments the source counter after hashing/storage.
A simple persistent map substitution would not preserve observable cache effects:
for example the inherited __proto__ setter ignores numeric hashes, so repeated
misses increment the counter repeatedly. The new provisional probes explicitly
exercise this rather than silently treating every string as an own cache slot.
Actual dynamic named-object storage/lookup/set semantics and scalar key handling
are next dependencies. Do not claim them based on existing fixed-schema Object
fields or substitute private invocation argument buffers for persistent collections.

Next rebase onto final reviewed PR99, acquire the released test slot, certify the
pin/native red, then adapt required object storage with source provenance and
executing checks. General prototype mutation/host interop and full public hash,
collection equality/hash, sequences and M2–M9 remain unfinished. PR97 exact
reviewed-head CI36823514952 remains live; no readiness claim until success.

## Cached string hashing verified red — 2026-10-01

PR97 is ready at independently reviewed8947aaa after final CI36823514952 SUCCESS.
Root inspected /private/tmp/suss-pr97-final-ci.log: Python76/reviews130+935/import47,
String3, copied-callback guard/ABI24 and every enabled full suite pass. No merge.
PR99 review pushed0fe9889 after72 exact fresh observations, independent malformed
value ABI recovery, Python76/provenance and full93986 success. Final CI36825310559
is live at that exact reviewed head; PR99 remains draft. No closure or milestone
acceptance. Updated both remote PR bodies with final review/CI evidence.

Rebased cache preparation onto PR99 final0fe9889, preserving both append-only
handoff histories; real pinned upstream clone is present. Root acquired the
explicitly released shared local slot. Fresh65884 ended101:all48 primary observations
match exactly, including prototype-named keys, repeated __proto__ misses, reset
thresholds, nil-before/after reset behavior, false/zero/NaN caches and live hasher
suppression. Native reports48 located unresolved string-hash-cache failures before
implementation; there are no native successes or skips. Log
/private/tmp/suss-string-cache-first-red.log. Command:
`CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2 sh scripts/test-string-cache-oracle.sh`.
The original48 source/expected entries are now certified by the pin and should be
preserved by future implementation/review. No cache forms were imported or falsely
marked implemented. All root local Cargo/JVM/Node processes are terminal.

Next implement owned dynamic named-object storage and the required native cache
lookup/set/factory adapters with explicit inherited/prototype key behavior, then
retain source cache declarations/add-to-string-hash-cache/hash-string with hashes,
EPL and focused execution. Existing fixed-schema Object fields are insufficient;
__proto__ numeric writes must not silently become ordinary own cache entries.
Full public js-obj arities/key conversion/prototype APIs require their own evidence;
do not infer them from a zero-argument cache factory. Persistent collections, full
public hash/equality, compiled macros and M2–M9 remain unfinished.


## Owned dynamic property kernel — 2026-10-01

PR99 final reviewed-head CI36825310559 completed SUCCESS at
0fe9889a0a05ba5ad44241cdbc293e2aeda93e1d. Root inspected the complete
/private/tmp/suss-pr99-final-ci.log, including review132/933, numeric3, ABI26
and all enabled workspace/doc suites. Updated its evidence body and marked
PR99 ready; no merge, issue closure or milestone acceptance.

Root continues in /private/tmp/suss-core-string-hash-cache on reviewed PR99.
Sixteen additional source object/prototype/key-order probes preserve the original48.
Fresh98282 ended101: all64 match the pin exactly, while native reports64 unresolved
string-hash-cache errors. No skips/success substitution. Log
/private/tmp/suss-string-cache-prototype-red.log; command uses the shared target,
build workers2 and sh scripts/test-string-cache-oracle.sh. This remains an
unpublished preparation branch; source cache/factory/prototypes are not implemented.

Added original native_objects.rs own-data kernel and two executing runtime tests.
Uses existing shared UserObject/header/GC array types, private appended descriptor
identity, raw UTF-16 property comparison and owned growth/replacement. No ABI/type
layout/core cell change, no registry and no persistent collection claim. Missing
own properties return Undefined; malformed pair/key/null-value storage and foreign
owners yield language exceptions. Prototype slot is reserved and unused here;
__proto__ is literal own data only until the prototype adapter intercepts it.
See docs/runtime/native-object-storage.md for exact scope and next dependencies.

First kernel test21837 passed. Added guard test initially failed compilation in
68500 and the immediately following full ABI attempt: an extra unwrap on a
Wasmtime Val was removed. Corrected focused90523 passed2 and full64457 passed28.
Null stored-value coverage21345 then failed because args-new supplies language nil,
not physical null; the test was corrected to forge an explicit null. Corrected
focused42885 passed2 and final ABI9964 passed28. Logs:
/private/tmp/suss-owned-properties-null-storage-corrected.log and
/private/tmp/suss-owned-properties-abi-final.log. Command:
`CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2 cargo test -p suss-compile --test runtime_abi --locked -- --test-threads=2`.
No full workspace run or independent review is claimed for this unpublished kernel.
All local processes are terminal; root retains the shared test slot for continuation.

Next add genuine prototype-aware lookup/set and inherited function values,
scalar key conversion, ignored primitive __proto__ writes, null prototype/data
shadow behavior and cycle rejection. Then wire the source factory/property adapters,
retain cache declarations/add-to-string-hash-cache/hash-string with pinned hashes
and EPL provenance, and require fresh64 native agreement, independent PR review,
full baseline and exact final-head CI. Do not narrow this to an own-property map
that changes pinned cache effects. Full public hashing/equality, persistent
sequences, compiled macros and M2–M9 remain unfinished. Deferred algorithm
question is issue98, outside existing milestone gates.


## Raw prototype chains — 2026-10-01

Continuation after14afd0e is concrete implementation progress. Added iterative raw
prototype read/set/chain lookup above owned data storage in native_objects.rs.
The previously reserved header prototype slot now holds nil or an owned object.
Complete candidate chains are checked before setter mutation; owner-containing
cycles are rejected atomically. Floyd cycle detection rejects host-forged cycles
without recursion, side registries or arbitrary chain depth limits. Chain lookup
uses present own slots, so an own Undefined shadows inherited values correctly.
This is original low-level storage code; no copied upstream forms or new source
provenance claim. No shared layout/ABI/core-cell change.

New focused tests exercise inherited values, own Undefined shadowing, self/ancestor
cycle rejection without changing the old chain, invalid prototype types, nil
removal, physically forged cycles/null slots, forced GC and recovery. The third
test executes a 130-object chain, rejects a tail-to-head cycle and recovers after
restoring its tail. No fake prototype functions or native-success substitutions.

First61843 ended101 on a test API typo (AnyRef::eq); changed to the existing
Rooted::ref_eq API. Second63503 ended101 because the test requested a nonexistent
undefined export; corrected the fixture to construct existing sentinel6 directly.
Corrected70972 passed2. Final focused62694 passed3, and complete ABI11110 passed31.
Logs /private/tmp/suss-native-prototype-final.log and
/private/tmp/suss-native-prototype-abi.log. Commands use shared target/build workers2
and cargo test -p suss-compile --test runtime_abi [runtime_abi_native_prototype]
--locked -- --test-threads=2. All local processes terminal. No full workspace,
independent review or final CI claim for this unpublished preparation branch.

Next implement the actual shared default Object prototype and callable inherited
methods, plus __proto__ accessor semantics and scalar key conversion. Raw prototype
set deliberately rejects primitives; the source inherited accessor must ignore
primitive writes and permit null removal/data shadows as certified by the pin.
Then wire source adapters and retain licensed cached hashing forms, preserving all64
certified observations. The full cache suite still has64 unresolved native failures;
these ABI tests do not establish cache/public js-obj compatibility. PR99 remains
ready at reviewed0fe9889/final CI36825310559 SUCCESS, no merge. M2–M9 remain active.


## Callable default Object prototype preparation — 2026-10-01

Continuation afterb5e8e45 implements original native_object_methods.rs and one
lazy shared GC root. Default-new objects share this root. Five actual closures
are installed: constructor/toString/valueOf/hasOwnProperty/isPrototypeOf. Member
methods use existing unbound Object wrapper invocation; no retained receiver or
placeholder for unfinished methods. Constructor accepts missing/nil/Undefined
or an existing owned object; primitive boxing remains explicitly unsupported.
Other object domains and Symbol.toStringTag are not claimed. New private mutable
root is appended; shared types/ABI version/core cells stay unchanged.

Validation12365 passed the existing3 prototype regressions on the new artifact.
New actual method invocation69798 passed1. A development Node check and the
ECMAScript Object prototype operations exposed incorrect detached isPrototypeOf
ordering: missing/primitive arguments return false before receiver validation;
object arguments throw for detached undefined this. Corrected both callback
paths and added regressions;92930 passed1. Final10496 passed1 with own nil presence,
inherited/own distinction, constructor identity/allocation, prototype identity,
member/detached semantics, forced GC and recovery. Complete ABI66488 passed32.
Logs /private/tmp/suss-default-object-methods-final.log and
/private/tmp/suss-default-object-methods-abi.log. Commands use shared target/build
workers2 with cargo test -p suss-compile --test runtime_abi
[runtime_abi_default_object] --locked -- --test-threads=2. Node evidence is
builtin development evidence, not fresh pinned ClojureScript source comparison.
All local processes terminal; root retains the test slot for continuation.

This unpublished preparation has no full workspace/review/final CI claim. The
source cache corpus remains64 unresolved native failures; no cache/native success
or milestone closure. Default prototype remains incomplete: actual remaining
methods, property attributes, inherited __proto__ getter/setter, immutable root
prototype rules and scalar property adapters are next. Then retain licensed
source cache forms, require fresh64 pinned/native agreement, independent PR review,
full required baseline and exact final-head CI. Do not infer complete public
js-obj/Object/hash behavior from these five bounded builtins. M2–M9 stay active.


## Scalar property/prototype accessor preparation — 2026-10-01

Continuation after4fd3f18 adds original native_object_properties.rs. Checked scalar
keys use existing coerce-string. The adapter preserves inherited default __proto__
getter/setter behavior, nearer own/inherited data shadows (including Undefined),
ignored supported primitive writes, null prototype removal followed by own-data
creation, cycle rejection and immutable default-root prototype changes. Same nil
root prototype writes remain allowed. Foreign object domains/physical null stay
language errors; no unknown-to-success path. No shared layout/ABI/core-cell change.
The raw storage still has no general property attributes/accessor descriptors.

78808 passed the existing default method test on the new validated artifact.
Added2 actual ABI regressions: scalar nil/bool/Undefined/-0/NaN/fractional/empty/
astral/lone-surrogate keys, __proto__ write/get/data shadows, inherited data, cycle/
root error atomicity and forced GC/recovery.99970 passed2. Complete ABI89189 passed
all34 existing tests, log /private/tmp/suss-native-property-abi.log. Focused log
/private/tmp/suss-native-property-first.log. Shared target/build workers2, command
cargo test -p suss-compile --test runtime_abi [runtime_abi_native_property] --locked
-- --test-threads=2. Independent development Node assertions also passed, log
/private/tmp/suss-proto-property-node.log. These are builtin runtime checks, not
fresh pinned ClojureScript/native cache comparisons.

Then added a meaningful desired-behavior regression for the discovered reflection
hole: runtime_abi_default_proto_accessor_is_an_own_property.11879 ended101 with
actualfalse(i31=2) versus expectedtrue(i31=4), while Node reports true. Log
/private/tmp/suss-default-accessor-descriptor-red.log. This failing test remains
active and unskipped. Therefore the current complete ABI suite is NOT green:
34 existing tests passed before adding the new failing regression. Do not infer
public Object compatibility or PR readiness from the prior34 result.

Next replace implicit root-identity accessor recognition with real owned property
descriptors/attributes and genuine accessor function values. Fix the new reflection
regression without special-casing hasOwnProperty, then implement remaining Object
methods and source adapters. Preserve the64 certified source corpus; native cache
forms still unresolved. No PR/full workspace/independent review/final CI claim on
this unpublished preparation branch. All local processes terminal. M2–M9 active;
no merge, closure or milestone acceptance.


## Owned data/accessor descriptors — 2026-10-01

Continuation afterb4c7d16 replaces implicit root-identity __proto__ recognition
with real owned descriptors [flags,payload], reusing existing GC arrays. Data
flags writable1/enumerable2/configurable4; accessor8 requires a pair of callable
getter/setter values or Undefined, with writable accessors rejected. All storage
checks descriptors before mutation; lengths/flags/null/malformed/noncallable cases
raise language exceptions. Integer flags are checked before i31 truncation.
Raw own-store/define are private internal redefinition operations, not public
Object.defineProperty semantics; configurable compatibility enforcement stays open.

Default root owns an actual configurable/non-enumerable __proto__ descriptor with
real unbound callable getter/setter wrappers. Generic property resolution/invocation
replaces the lookup special case. Nearer descriptors shadow farther ones; receiver
is preserved for arbitrary descriptor keys. Readonly data and setter-less accessors
ignore writes in the bounded non-strict adapter; writable own data retains flags,
and writable inherited data creates ordinary ownflags7. Builtin methods now have
proper non-enumerable/writable/configurable flags5. Added real propertyIsEnumerable
and toLocaleString (live property lookup/invocation). No placeholder methods.
No shared type/layout/ABI/core-cell change. Root remains lazy GC-owned.

Original desired reflection regression80735 passed1 unchanged. Existing property
regressions passed2, log /private/tmp/suss-descriptor-property-regressions.log.
9863 passed existing default method1. New descriptor20307 passed2: readonly own/
inherited writes, ordinary new property flags, copied getter/setter receiver,
invalid signed/truncated flags, malformed payloads, enumerability and live toString
returning the actual receiver through valueOf. Complete ABI38225 passed37, log
/private/tmp/suss-owned-descriptors-abi.log; focused log
/private/tmp/suss-owned-descriptors-first.log. Commands use shared target/build
workers2 and cargo test -p suss-compile --test runtime_abi [descriptor] --locked
-- --test-threads=2. All active tests enabled; previous own-accessor failure fixed.
Independent development Node assertions pass, log
/private/tmp/suss-owned-descriptors-node.log. No fresh pinned source comparison
or full workspace/review/final CI claim for this unpublished preparation.

Next implement remaining real legacy define/lookup getter/setter methods with
receiver/key/callability/descriptor compatibility checks, then wire source adapters
and licensed cached hashing forms. Public define/delete, primitive boxing, foreign
object domains and Symbol.toStringTag remain open. Preserve64 certified source
observations; native cache forms still unresolved, not acceptance. All processes
terminal. M2–M9 goal active, no merge/closure/milestone acceptance.


## Real legacy accessor methods — 2026-10-01

Continuation aftereae27c8 adds all four real default legacy define/lookup getter/
setter methods. Eleven function values and one actual __proto__ accessor now
populate the owned default prototype. Define checks receiver/callback callability
before scalar key conversion, rejects non-configurable own properties, preserves
an existing own accessor counterpart and defines flags14. Lookup resolves stored
descriptors through the chain and returns actual getter/setter identity without
invocation; nearer data shadows stop lookup. Internal selector bounds reject
negative/out-of-range values before array access. No placeholder functions.
No shared type/layout/ABI/core-cell change.

First formatter/Cargo attempt ended101 on a mismatched closing delimiter in the
new helper; fixed that syntax before execution. Corrected93893 passed2 existing
descriptor tests. New66004 passed2 actual legacy regressions: member invocation,
inherited getter/setter lookup, receiver identity, counterpart preservation,
setter-only Undefined reads, flags14, data shadowing, detached throws, typed
rejection/GC recovery and configurable data conversion versus non-configurable
atomic rejection. Full ABI75670 passed39. Logs
/private/tmp/suss-legacy-accessor-first.log and
/private/tmp/suss-legacy-accessor-abi.log. Commands use shared target/build workers2
and cargo test -p suss-compile --test runtime_abi [runtime_abi_legacy_accessor]
--locked -- --test-threads=2. Independent development Node assertions also pass,
/private/tmp/suss-legacy-accessor-node.log. Not a fresh source oracle comparison.

Next wire private compiler object factory/get/set operations, retaining source
js-obj/cache declarations/add-to-string-hash-cache/hash-string with source hashes
and EPL. A function-valued adapted js-obj factory must preserve first-class/live
binding behavior and variadic argument evaluation before odd-arity errors; do not
silently advertise a zero-argument-only public factory. General public definition/
deletion, primitive boxing/foreign objects and Symbol.toStringTag remain open.
Preserve64 source observations, require fresh native agreement then independent
PR review/full baseline/exact final-head CI. No PR readiness/merge/closure or
milestone claim on this unpublished branch; all processes terminal. M2–M9 active.


## Compiled private object adapters and variadic factory — 2026-10-01

Continuation after99078a6 adds original native_object_factory.rs and checked
Nominal HIR operations NativeObjectFactory/Get/Set. Direct private suss.bootstrap
object-factory returns a real function; object-get/set lower evaluated operands
in order with typed runtime imports. Private wrong arities fail during analysis,
atomically. HIR and IR use existing Nominal validity checking, not the scalar
bitwise enum. No public core names/source forms/ABI layout/core cells changed.

Factory invocation accepts evaluated variadic arguments and rejects odd pairs
at runtime, preserving effects before failure. Empty/even pairs create default
owned objects through descriptor-aware property writes. Single native-array
arguments flatten recursively using checked source-array storage. An iterative
Floyd check rejects self/mutual array cycles without arbitrary depth limits or
recursion. Factory is a function value, not a zero-argument-only public claim.
No singleton factory registry retains owners; live vars/captures use normal cells.

Initial96061 passed existing default method1 on the validated new artifact.
New compiled source63175 passed2; final52318 passed2 with capture/rebinding probes.
Source executes empty/pairs/nested arrays, effects12345, inherited properties,
scalar keys, runtime odd-arity effects, private compile-atomic arities, malformed
receivers/keys, self/mutual native-array cycles and post-GC recovery. Log
/private/tmp/suss-native-object-source-final.log. Command uses shared target/build
workers2, cargo test -p suss-cli --test portable_native_objects --locked --
--test-threads=2. New ABI22872 passed1 for foreign/null next/flatten buffers and
forged null keys, typed exceptions/recovery. Complete ABI48069 passed40, log
/private/tmp/suss-object-factory-abi.log. No skips or unknown-success substitution.

Next retain runtime js-obj/cache declarations/add-to-string-hash-cache/hash-string
and macro unchecked-get/set with explicit patches/hashes/EPL. Respect function-
valued public factory arities/live cells and original source counter/reset ordering;
macro property operands must preserve evaluation order. Preserve64 certified source
observations and certify fresh primary/native agreement. No full workspace/PR
review/final CI claim for this unpublished branch. Source cache64 remains unresolved
until forms are retained; full public foreign-object/boxing/descriptor definition/
delete/Symbol behavior and M2–M9 remain open. All local processes terminal.


## Retained cached string hashing first green — 2026-10-01

Continuation after2a66db4 retains5 runtime forms with explicit patches/original
metadata/EPL: js-obj/cache/cache-count/add-to-string-hash-cache/hash-string. Cache
source order and live dependencies retained. Added bounded checked unchecked-get/
set macro lowering. Macro literal js-obj and general foreign/array/string bracket
interop remain explicitly incomplete. Original native named member wrapper routes
owned objects to descriptors while preserving fixed nominal/String/Array fallback.
No shared type/layout/ABI/core-cell change. Source cache uses Object descriptors,
not persistent maps. Recipe50/import54 and reviews139/926 remain partial.

Native first91365 passed64 observations. Fresh primary67283 passed64 exact pin
observations and native1, log /private/tmp/suss-cache-fresh-primary.log, command
shared target/build workers2 sh scripts/test-string-cache-oracle.sh. Original48
and all64 certified expected/source entries unchanged. Final native98787 passed3
with aliases, first-class variadic factories, native members, own accessor reflection,
mutable named property, captured/live hasher dependencies, lexical shadowing,
compile-atomic macro arities/runtime core arities/effect-before-error/recovery.
Log /private/tmp/suss-cache-source-final.log. Python/provenance5116 ended0:
Python76/inventory1065/reviews139+926/import54. Log /private/tmp/suss-cache-python.log.
Final ABI36609 passed40, /private/tmp/suss-cache-abi-final.log. No skips or false
milestone success. Commands cargo use --locked/--test-threads=2/shared target and
build workers2.

Required full workspace baseline29258 is running, confirmed live at launch; log
/private/tmp/suss-cache-full-baseline.log. Root exclusively owns heavy local test
slot until that handle is terminal. No independent PR review or final-head CI
claim yet. Next inspect terminal full result, open the coherent cached hashing PR
with Refs9/11/14/16/19, dispatch independent review/fixes and require exact reviewed-
head CI. Do not merge. Public hashing/equality, actual persistent sequences/maps,
compiled macros, complete host-property operations and M2–M9 remain unfinished.


## PR100 independent review — 2026-10-01

Root baseline29258 finished successfully before review in
/private/tmp/suss-cache-full-baseline.log. Draft PR100 was opened from3fbabd1,
base reviewed PR99/0fe9889; independent reviewer works in
/private/tmp/suss-review-pr100. Root released the exclusive heavy test slot after
29258 became terminal; reviewer runs no parallel Cargo/JVM/Node processes.

Review found two significant strictness defects in the retained Closure operations.
The pinned Closure object module is strict: gobject/set and gobject/create pair
assignment throw for getter-only or non-writable resolved properties. The candidate
used the same non-strict store as unchecked-set, silently ignoring blocked writes;
cached hashing then incorrectly incremented the counter. Fresh primary68 attempt
39032 ended1 on an uncaught getter-only write while the initial native probe had
returned2. After splitting the store, first focused83360 ended101 because that
new provisional native expectation still said2; actual0 correctly preserved the
counter. Corrected this new expectation only, then fresh48006 certified68 exact
pin observations/native4. Fresh59337 certified69 exact pin/native4, including
non-strict unchecked-set ignoring a getter-only write. Original64 remain unchanged.

Added strict private object-set-strict/typed runtime import and descriptor-aware
strict store. Getter-only and readonly writes raise language exceptions before
mutation; arbitrary setters retain actual receiver-aware invocation. Retained
add-to-string-hash-cache uses the strict store; counter/reset order and live
bindings stay unchanged. Runtime factory pair assignments also use strict store,
matching gobject/create. Source patch rationales, hashes/manifest, manual dependency
review and stale storage documentation were corrected. Shared GC layout, ABI
version and core-cell count stay unchanged; public defineProperty remains open.

Focused ABI11481 passed1: own/inherited readonly data throws typed exceptions,
not traps, preserves data and recovers through GC; writable inherited data creates
an own property. Pre-factory-fix full93776 passed, log
/private/tmp/suss-pr100-review-full-baseline.log. An additional first-class factory
probe avoids the separate unfinished literal macro. Fresh21556 certified70 exact
pin observations then ended101 because native returned an Object instead of throwing
and returning caught Number41; strict decoder rejected the wrong layout. After
factory fix, fresh92625 passed all70 exact pin observations and native4, log
/private/tmp/suss-pr100-review-primary70-final.log. New cases also certify inherited
getter effects, inherited setter without own-data creation, throwing cache writes,
getter-only counter atomicity and non-strict bracket behavior. All original64
sources/expectations were checked against3fbabd1 and remain unchanged.

Commands use shared target /Users/bobby/code/github/bobby/suss/target and build
workers2: sh scripts/test-string-cache-oracle.sh; cargo test -p suss-compile
--test runtime_abi runtime_abi_strict_owned_store --locked -- --test-threads=2;
cargo test --workspace --locked -- --test-threads=2. Final independent Python76,
inventory1065/reviews139+926/import54 pass, log
/private/tmp/suss-pr100-review-provenance-final.log. No skipped success substitution.
Final workspace79254 is live at launch, log
/private/tmp/suss-pr100-review-full-final.log; terminal result will be recorded below.
Exact reviewed-head CI remains root's next readiness gate after reviewer push.
No merges or issue/milestone closures. Public hashing/equality, persistent sequences,
collections, literal js-obj macro, full property domains and M2–M9 remain open.

Final workspace79254 ended0: all required workspace suites and doc tests pass,
including source cache4 (70 corpus observations), private source2 and ABI41.
Log /private/tmp/suss-pr100-review-full-final.log. All reviewer local processes
are terminal and the exclusive heavy slot is released to root. Independent review
has no remaining significant findings in this bounded slice after the two strict
Closure fixes. Reviewer pushes this reviewed source/evidence head to PR100; root
must require exact reviewed-head CI before readiness. Next unblocked implementation
is persistent sequence/list foundations, while literal macro/property domains and
full hashing/equality remain tracked unfinished. No merge or milestone completion.

## Sequence/list preparation and canonical literal — 2026-10-01

New isolated branch portable/core-sequence-foundation at candidate PR100
3fbabd1924ce4e39d29ad3e6dd949b11a40c78e1 preserves the six original preparation
artifacts from7b60562 without replaying its stale parent stack or overwriting
current roadmap/ABI evidence. All original48 sequence source/expectation JSON
values are preserved. Three newly appended static-property candidates bring the
corpus to51; they are not freshly certified yet. The native test explicitly
loads the generated core artifact and remains expected red on unresolved seq.

Pinned compiler.cljc577–580 emits cljs.core.List.EMPTY for (). Portable HIR now
resolves canonical suss.core/List and performs the existing checked named read.
No runtime type/layout/bootstrap change. Added one original adapter regression
for GC, canonical alias/lexical/user List shadowing, captured reader and live
static-property replacement. Preserved all five preexisting core interface tests;
the older preparation worktree's short test file was not used to replace them.
This is a compiler prerequisite, not concrete List/EmptyList acceptance.

Python inventory_records validated the pin and recorded20 complete source
ranges/hashes in docs/runtime/sequence-source-audit.json. git diff --check passes.
Rustfmt was limited to the two sequence test files. No Cargo/JVM/Node execution
on this branch yet: independent PR100 reviewer exclusively owns the shared heavy
slot. No new PR/readiness/closure or milestone completion claim.

PR100 review found strict Closure gobject/set and gobject/create writes were
incorrectly using non-strict bracket stores. Reviewer is preserving bracket
semantics and certifying separate strict stores, including cache counter effects
and first-class factory behavior. Candidate100 CI36832808888 was confirmed live
in_progress at3fb. Candidate CI cannot certify a changed reviewed head. Root
must inspect reviewer terminal results/release, rebase this branch onto the pushed
reviewed100 head, run focused core_sequence_interfaces and fresh sequence oracle
(record native red transparently), then retain actual upstream list/sequence types
with their full dependency closure. Require independent review/full/final-head CI
for every eventual PR. Never merge. M2–M9 remain active and incomplete.


## Sequence canonical lookup executed / fresh51 red — 2026-10-01

Independent PR100 review finished and pushed3e5af026f5b2f0de09edace672ad0f45c084e93a.
Root inspected final workspace log /private/tmp/suss-pr100-review-full-final.log
and reviewer handoff; full79254 is terminal0, sourcecache70/private2/ABI41 and
Python76/inventory1065/reviews139+926/import54 pass. Two significant strict Closure
findings are fixed; no remaining significant finding. Updated PR100 body with
actual review/fixes/results. Exact-head CI36834509263/job110278870352 is confirmed
in_progress at3e5af02. Candidate36832808888 is cancelled and cannot gate readiness.
PR100 stays draft; next require this exact CI success before marking ready.

Rebased sequence preparation2d44d8b onto reviewed3e5af02; resolved only appended
handoff conflict by preserving both histories, producing53281ca. Pinned source
is a detached nested Git worktree atc4295f30, not a tracked symlink. No unrelated
root files/stashes changed. All original48 source/expectation JSON values were
checked against7b60562 and remain equal;3 candidates are now independently certified.

Focused core_sequence_interfaces10925 ended0 with6 passing tests. Extended the
canonical literal regression with compile-atomic premature definition failure
and live nil EMPTY reads, then final19497 ended0 with6/6 passing. Log
/private/tmp/suss-sequence-canonical-final.log. Command: shared target/build2
cargo test -p suss-cli --test core_sequence_interfaces --locked -- --test-threads=2.

Fresh sequence18929 ended101: primary51 all match exactly; native explicitly
loads core then fails on unresolved Runtime name seq at7..10. Log
/private/tmp/suss-sequence-primary51-native-red.log. Command: shared target/build2
sh scripts/test-sequence-oracle.sh. Recorded failure is the actual next implementation
boundary; no ignored test/expectation substitution/native sequence success claim.
All root local processes terminal. Full workspace is not claimed green for this
unfinished sequence branch, which contains the deliberate native red regression.

Next retain actual upstream List/EmptyList/Cons/IndexedSeq and their audited
source dependency closure, publish the real EmptyList on List.EMPTY, implement
seq APIs and general persistent variadic rest. Preserve51 observations and certify
native execution with equality/hash/metadata/reduction/iterator/lazy obligations
still in scope. Do not replace concrete types with the original literal test adapter.
No new PR yet; require independent review/fixes/full/final-head CI before readiness.
M2–M9 stay active; no merges or issue/milestone completion claims.


## Retained concrete sequences and persistent variadic rest — 2026-10-01

PR100 is READY, still unmerged: head3e5af026f5b2f0de09edace672ad0f45c084e93a,
independent review/fixes and full79254 passed. Exact reviewed-head CI36834509263
completed SUCCESS; root inspected /private/tmp/suss-pr100-final-ci.log, updated
PR body and ran gh pr ready100. No merge/closure/milestone acceptance.

Sequence branch continues from0f60113. Retained all methods of List/EmptyList/Cons/
IndexedSeq plus14 helper/API declarations:18 new selected forms bring import68/
72 licensed artifacts, reviews157 partial/908 unassessed. Explicit patches retain
algorithms/docstrings/metadata/EPL and replace host Error/numeric array append/
bounded defn. Empty ordered hash's unsupported private attribute is omitted only
in its explicit executable patch; privacy/Var metadata remain pending. No blanket
method deletion or fake helpers. Original loader forward declarations explicitly
name pending print/index-search/equality/hash/reduction/iterator/RSeq/str_ helpers;
invoking an uninitialized dependency fails, not succeeds. Two complete licensed
setup statements publish real List.EMPTY and nil ICounted. New hash-bound loader
inputs/manifest and scripts/sequence_provenance.py plus3 regressions verify source
bounds/hashes/notices/order and reject edits/extra forms. CI runs that check.

Compiler canonical () lookup was retained. HIR/verified IR now carry variadic
method/rest-class facts. Exact fixed dispatch precedes a variadic entry. Rest
uses a fresh source array and live canonical IndexedSeq class construction with
array/offset0/nil metadata. No raw Args masquerades as a persistent sequence.
Empty rest is nil and skips class lookup/construction. Named self/recur, captures,
source evaluation order, arity checks and malformed signature atomicity execute.
Private language-error-new uses existing descriptor/Exception layout and checked
UTF-16 coercion. No shared type/layout/ABI/bootstrap core-cell change.

First type focus38038 ended101 on unsupported private metadata; explicit source-
bound empty hash patch corrected loading and47239 passed6 interface tests. New
fixture failed compilation on nonexistent StructRef.get; corrected strict fields
before42637 passed2 actual type/error tests. Initial variadicbuild12819 passed2.
Initial list15567 passed all51/native3. First fresh67/25453 ended1: one new candidate
wrongly expected array-seq to supply rest. Pinned compiler.cljc987–1001/1058–1074
constructs IndexedSeq directly. Corrected only that new expected7/renamed probes,
added class/empty-rest probes and fixed compiler binding/construction accordingly.
All original51 checked JSON-equal against0f60113 and untouched. Fresh9486 certified
69 exact primary observations/native3. Final source98359 passed4 with persisted
rest/GC/wrong-arity effects/malformed signatures/recovery; final72919 repeats69 exact
primary/native4, /private/tmp/suss-sequence-primary69-final.log.

Python loader13 initially passed; source setup provenance metadata initially listed
source order while the explicit loader runs singleton then nil extension. Corrected
metadata to actual retained statement order; all80 Python tests pass, plus inventory/
reviews157+908/import72/setup2. Log /private/tmp/suss-sequence-python-final.log.
Commands: python3 -m unittest discover -s scripts -p test_*.py; python3 scripts/
sequence_provenance.py; python3 scripts/core_import.py --check; python3 scripts/
cljs_reviews.py. Rustfmt only test/new modified function regions, not workspace.

First full72012 ended101 on an old unsupported-variadic fixture expecting span&;
without loaded source core, the new lowering correctly reports missing IndexedSeq
at the function span. Updated that regression to assert exact dependency diagnostic,
retaining destructuring/arity failures. Closure27762 passed13; added malformed
variadic IR class/arity/entry rejection and final41525 passes14. Pinned unchecked-
max binds operands once; corrected the count patch to explicit let/branch before
final fresh72919. No unchanged oracle result retargeting or skipped success.

Final full workspace6028 running, confirmed live through portable pipeline tests.
Log /private/tmp/suss-sequence-full-final.log. Root exclusively owns heavy slot;
all earlier local handles terminal. Cargo commands always shared target, build2,
--locked/--test-threads=2, no RUSTFLAGS. Required baseline before opening coherent
sequence/rest draft PR against100 with Refs9/14/16/17/19. Then dispatch independent
subagent review/fixes, reviewer full baseline and exact reviewed-head CI. Do not
merge. Complete helper dependencies, equality/hash/metadata/reduction/iterators,
general apply and compiled macros remain next; lazy/chunked/vector/HAMT/transient
and M2–M9 acceptance stay unfinished. Source seq JS Symbol.iterator branch is
outside this portable adapter; typed bridge/iterators and str_ formatting remain
explicit pending boundaries. No full core/collection or milestone completion.


Final full6028 ended0; root inspected /private/tmp/suss-sequence-full-final.log
through final doc-test completion. All required workspace suites pass, including
native sequence4/fresh69 cases, interface6, closure14 and runtime ABI41. Existing
manual ignored observations and diagnostic9 passing/7 known failures remain
explicit. No scope-wide compatibility claim. Original51 JSON values checked
against0f60113 remain identical. All local handles terminal; root may open the
coherent sequence/rest draft and release the heavy slot to its mandatory independent
reviewer. Exact final reviewed-head CI still gates readiness. No merges.


## Independent PR101 review — 2026-10-01

Reviewed candidate4d5ee493771768b2ac841b9384895f189fce757d independently in
/private/tmp/suss-review-pr101 against reviewed PR100 base3e5af02. Read accepted
design/ROADMAP/inventory/handoff and inspected complete source types, every
adaptation, setup/EPL/provenance, canonical empty lookup, typed Error, and
fixed/variadic HIR/IR/dispatch/captures/recur. No significant production defect
identified in this bounded foundation. Wrong arity remains a typed diagnostic per
accepted design; unchecked JS calls are not a substitute for that contract.

Independent fresh51409 ended0 with all original69 exact pinned/native matches.
Added six edge observations without changing any original source/expectation;
fresh59125 ended0 with75 exact primary/native matches and all5 native sequence
tests. Log /private/tmp/suss-pr101-review-oracle75.log. New probes cover fixed0 vs
variadic0 dispatch, many arguments, recur with a List rest, separate owned rest
arrays, array growth and captures/named self calls. Additional native test rebinds
IndexedSeq to nil/false/number/function: empty rest bypasses the class; nonempty
rest raises Language error after argument effects; GC plus restoration recovers.
No trap, silent success, oracle retargeting or ignored failure.

Command: CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target
CARGO_BUILD_JOBS=2 sh scripts/test-sequence-oracle.sh. Python80 passed,
/private/tmp/suss-pr101-review-python.log; both licensed setup forms, import72 and
reviews157 partial/908 unassessed verified using python3 -m unittest discover -s
scripts -p 'test_*.py', sequence_provenance.py, core_import.py --check and
cljs_reviews.py. Original69 JSON values verified identical to candidate4d5ee49;
original corpus text retains its escaping, with additions only.

Independent full workspace session70659 ended0; inspected log through all
final doc tests, /private/tmp/suss-pr101-review-full.log. All required suites pass;
existing manual ignored and explicit diagnostic gaps remain unchanged. Command: shared target/build2
cargo test --workspace --locked -- --test-threads=2. No RUSTFLAGS. Exact final
reviewed-head CI required after review commit/push; PR101 remains draft, no merges.
Next implement the explicit pending source helpers for printing/sequential
hash/equality/reduction/iterators/reversal/general apply, then lazy/chunked/vector/
HAMT/transient acceptance. Full lossless collection decoder and M2–M9 gates remain
unfinished; no issue or milestone closure.

## Retained sequential equality — 2026-10-01

PR101 independent review pushed47170abf51e4b0ef927cd9af323e6919a4f822f7,
no significant production finding. Fresh75/native5/Python80/full70659 terminal0;
reviewer released heavy slot after all handles terminal. Root verified exact-head
CI36840696723 live; draft remains until successful final reviewed-head CI.

Separate branch portable/core-sequential-equality based on reviewed101 retains
pinned =1342/equiv-sequential3151 and complete unmodified number/default IEquiv
extensions1475/1866. Selection70/artifacts74/reviews159partial+906unassessed;
source setup verifier4, forward declarations/hash-bound loader and EPL unchanged
except reviewed new forms. No new runtime ABI/type/bootstrap-cell/algorithm.

Initial native36319 terminal0, original75 match/native5 pass. Added56 candidates.
Fresh84867 ended1: new provisional user-def equality expectation contradicted
pin (user shadow leaves core helper intact). Corrected only that new expectedfalse
and added qualified core-cell probe. Fresh21347 ended1 with pinned direct-arity2
TypeError on fixed replacement; corrected new probe to multi-arity replacement.
Final20778 terminal0 fresh131 match/native5, log
/private/tmp/suss-equality-primary131-final.log. All original75 source/expectation
objects preserved. Added wrongarity/effects/typed user throw/saved equality/GC
recovery: native19043 terminal0 six tests, /private/tmp/suss-equality-native6.log.
Python80 pass, /private/tmp/suss-equality-python.log; import/setup4/reviews checks
and diff-check pass. No changed certified result, skip, broad acceptance claim.

Required full2502 live, /private/tmp/suss-equality-full.log, root exclusively
owns heavy slot. Command shared target/build2 cargo test --workspace --locked --
--test-threads=2; no RUSTFLAGS. Read final output before commit/push/draft PR against
101. Mandatory independent review/fixes/full/exact final-head CI gate readiness;
no merge. Remaining retained hash/reduced/reduction/iterator/reversal/printing/
index-search helpers, other persistent types and compiled macros are next;
M2–M9 and issue IDs remain active, no closures.

Full2502 terminal0; root inspected final doc-test completion in
/private/tmp/suss-equality-full.log. Required workspace baseline passes with
native sequence6/closure14/ABI41 and all enabled required suites. Existing manual
ignored tests and diagnostic9passing/7knownfailures remain explicit; no broad
compatibility acceptance. All root local handles36319/84867/21347/20778/19043/
32175/2502 terminal, including the two preserved primary failures above. Ready
for candidate commit/push/draft PR and mandatory independent review. Root must
release heavy slot to that reviewer; exact reviewed-head CI still gates readiness.


## Independent PR102 review — 2026-10-01

Reviewed candidatee893b1db61f16d1ae89bc076c9f834016852ae38 against reviewed
PR101 base47170ab in /private/tmp/suss-review-pr102 with actual detached pinned
upstreamc4295f30. Read accepted design/ROADMAP/inventory/handoff; compared complete
retained equality algorithms/arities/docstrings/metadata and source setup statements,
defn patches/provenance/EPL and native/direct protocol dispatch. No significant
production defect identified. Privacy/runtime Var metadata and compiled macros
remain explicitly incomplete, as do remaining collection types and public hashing.

Independent fresh16559 ended0: original131 exact primary/native6. Preserved every
original131 source/expectation object and added six edge probes. The original
CountProbe uses default identity and cannot alone establish counted rejection;
new direct helper and actual IEquiv delegation prove count calls in order12 and
no seq effects. New uncounted length/direction/variadic short circuit/zero/NaN cases
also pass. Fresh18729 ended0 with137 exact pinned primary/native6, log
/private/tmp/suss-pr102-review-oracle137.log. Native arity recovery now checks each
intermediate effect, rather than only final29. Python9589 ended0,80 passed,
/private/tmp/suss-pr102-review-python.log; import74/setup4/reviews159partial+906
unassessed and diff-check pass. No certified expectation retargeting or skips.

Commands use shared target/build2, no RUSTFLAGS: sh scripts/test-sequence-oracle.sh;
python3 -m unittest discover -s scripts -p 'test_*.py'; core_import.py --check;
sequence_provenance.py; cljs_reviews.py. Required full workspace63890 remains
running. Reviewer owns heavy slot until all handles terminal and latest regression
validation/full baseline inspected. Exact final reviewed-head CI still gates PR102
readiness; do not merge or close milestones. Next retained reduced/reduction,
ordered/public hashing, iterators/reversal/printing/index helpers and other
persistent types remain unblocked M4/M7 work; M2–M9 acceptance incomplete.


Independent first full63890 ended0 through final doc tests,
/private/tmp/suss-pr102-review-full.log. After strengthening intermediate native
arity effects, focused68289 ended0 with six tests/137 corpus cases,
/private/tmp/suss-pr102-review-native-final.log. Final required latest-source full
90258 ended0; inspected final doc-test completion in
/private/tmp/suss-pr102-review-full-final.log. Command: CARGO_TARGET_DIR=/Users/
bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2 cargo test --workspace
--locked -- --test-threads=2. All enabled required suites pass; existing manual
ignores and diagnostic9passing/7knownfailures remain explicit. All reviewer local
handles16559/18729/9589/63890/68289/90258 terminal. Reviewer releases heavy slot
after committing/pushing this evidence and tests; root must verify successful
exact reviewed-head CI before readiness. No merge or milestone closure.

## Sequence reduction preparation — 2026-10-01

Isolated branch portable/core-sequence-reduction from candidate102 prepares
retained IDeref/Reduced/reduced helpers/deref/array-reduce/iterable?/iter-reduce/
seq-reduce/reduce plus original bounded symbol-binding if-let lowering. All
upstream reduction algorithms/methods are retained; selected82/generated86 and
172 partial/893 unassessed are provenance/preparation counts only. New59 reduction
and18 control candidates are UNVERIFIED, not certified matches. No native/full
pass claim for this preparation. Reviewer102 owned heavy slot during preparation.

Reviewer102 now pushed a9f19e8650c1c2079e2c5066ebde076f9ea284a8 with evidence
fixes; fresh137/native6/Python80/full90258 terminal0, all handles terminal and
heavy slot explicitly released to root. Root must rebase preparation onto that
reviewed parent before focused loading/if-let/primary/native/full verification.
Exact102 reviewed-head CI required before readiness; no merges/issue closures.
No RUSTFLAGS; shared target/build2 and test threads2. Next execute the prepared
reduction artifact, correct actual failures with source/order/provenance aligned,
then independent review/fixes and exact final-head CI for a coherent PR.

Preparationa1f579a rebased onto reviewed102a9f19e8; only handoff append conflicted,
resolved retaining both complete review and preparation sections. Source reduction
and if-let architecture follows the pin; no ABI/type/core-cell change. Initial
focus34984 terminal101: standalone control harness lacks source inc/dec, so new
candidates use +/-, no expectation changes. Focus24330 terminal101: control3 pass,
new native number IReduce case returns NaN. Fresh89149 terminal1 confirms pin
also returns NaN and warns base-type arities must be grouped; no production bug.
Corrected only new candidate extension shape. Fresh1985 terminal0 with59 exact
primary/native, /private/tmp/suss-reduction-primary59-final.log. Control16870
terminal0 with77 exact primary/native3, /private/tmp/suss-reduction-control-primary77.log.
All original59 control and137 reviewed sequence cases preserved.

Added saved reducer/Reduced/GC/arity-effects/arbitrary typed throw/recovery and
if-let aliases/exclusions/compile atomicity guards. Native50993 terminal0 with
reduction2/control3, /private/tmp/suss-reduction-native-final.log. Python32282
terminal0 with80 passes, /private/tmp/suss-reduction-python.log; import86/setup4/
reviews172partial+893unassessed/diff-check pass. Rustfmt only new if-let arm and
changed/new native test files, no unrelated formatting. Primary failures preserved.

PR101 READY, still unmerged: reviewed47170abf51e4b0ef927cd9af323e6919a4f822f7;
exact-head CI36840696723 SUCCESS completed2026-10-01T09:31:05Z. Root inspected
/private/tmp/suss-pr101-final-ci.log through final doc tests, updated body and
ran gh pr ready101. PR102 revieweda9f19e8 remains draft while CI36842775585 live.
No issue closure/milestone acceptance.

Full46990 running, /private/tmp/suss-reduction-full.log. Root exclusively owns
heavy slot; all prior root handles terminal. Required shared-target/build2 cargo
test --workspace --locked -- --test-threads=2, no RUSTFLAGS. Inspect terminal
baseline before final commit/push/draft PR against102, independent subagent
review/fixes/full and exact reviewed-head CI before readiness. No merge. Next
retained ordered/public hash, iterator/reversal/printing/index helpers plus remaining
persistent types and compiled macros; lazy/chunked/transducer/metadata/transient
and M2–M9 release acceptance remain incomplete.

Full46990 ended0; inspected /private/tmp/suss-reduction-full.log through final
reader doc tests. All required workspace suites pass, including reduction2/control3
and prior sequence6/137, closure14 and ABI41. Existing manual ignores and
explicit diagnostic9passing/7knownfailures remain unchanged. No scope-wide
compatibility or milestone acceptance. All root local handles34984/24330/89149/
1985/16870/32282/50993/46990 terminal; initial failures retained above. Ready to
commit/push/open coherent reduction draft against102 and release heavy slot to
mandatory independent reviewer. Final reviewed-head CI still gates readiness;
no merges or closures.

## Independent PR103 review — 2026-10-01

Reviewed draft PR103 candidate0d74c4e3448cb419e3927969330792d9a7af9c5c against
reviewed PR102 basea9f19e8650c1c2079e2c5066ebde076f9ea284a8 in isolated
/private/tmp/suss-review-pr103 with actual detached pinned upstreamc4295f30.
Read accepted design/ROADMAP/inventory/handoff/AGENTS. Inspected complete retained
IDeref/Reduced/helper/reduction forms, every defn adaptation, methods/arities/
metadata/docstrings/EPL/provenance, dispatch ordering and Reduced stop semantics.
Inspected original if-let bootstrap resolver/HIR, fresh binding identity, metadata,
source spans, scope restoration, source evaluation order and caller tail context.
No significant production defect identified in this bounded source dependency slice.
Full compiled macros/destructuring/runtime Var metadata and surrounding M2–M9
acceptance remain incomplete; no merge or issue/milestone closure.

Independent original fresh reduction51369 ended0:59 exact pinned/native2;
control24238 ended0:77 exact pinned/native3. Added six reduction and four control
observations, preserving all original59/77 source/expectation objects and the
parent137 sequence observations. Fresh63450 ended0 with65 exact reduction/native2,
/private/tmp/suss-pr103-review-reduction65.log; fresh55771 ended0 with81 exact
control/native3, /private/tmp/suss-pr103-review-control81.log. New cases establish
iterator hasNext/next stopping order, direct IReduce before IIterable, Reduced
identity for singleton/empty arrays, offset array captured length, nested Reduced
iterator unwrapping, and nested/captured if-let scope/effect order. No expectation
retargeting, failure skips or production changes. Original JSON text/escaping was
preserved while appending cases. Compiler HIR regression97105 ended0 and verifies
exact source spans, reader metadata and distinct scoped binding identities;
/private/tmp/suss-pr103-review-iflet-hir.log.

Python70561 ended0:80 tests pass, /private/tmp/suss-pr103-review-python.log;
import86/setup4/reviews172 partial+893 unassessed verified. Diff-check passes.
Commands: shared target/build2 sh scripts/test-reduction-oracle.sh and
sh scripts/test-control-flow-oracle.sh; cargo test -p suss-compile --test
portable_if_let --locked -- --test-threads=2; python3 -m unittest discover -s
scripts -p 'test_*.py'; core_import.py --check; sequence_provenance.py;
cljs_reviews.py. No RUSTFLAGS, no unrelated formatting/files.

Required independent full40341 live, /private/tmp/suss-pr103-review-full.log.
Reviewer owns exclusive heavy slot until all handles terminal and final full output
inspected. Exact reviewed-head CI remains root's readiness gate after review
commit/push. No merge. Next retained ordered/public hashing, iterator/reversal/
printing/index helpers, remaining persistent types and compiled macro acceptance;
lazy/chunked/transducer/metadata/transient and all release gates remain unfinished.

Independent full40341 ended0; reviewer inspected all final doc-test completion in
/private/tmp/suss-pr103-review-full.log. Command: CARGO_TARGET_DIR=/Users/bobby/
code/github/bobby/suss/target CARGO_BUILD_JOBS=2 cargo test --workspace --locked
-- --test-threads=2. All enabled required suites pass, including reduction2/65,
control3/81, HIR if-let1, parent sequence6/137, closure14 and ABI41. Existing
manual ignored tests and diagnostic9passing/7knownfailures remain explicit.
All reviewer local handles51369/24238/63450/55771/70561/97105/40341 terminal.
Reviewer releases heavy slot after committing/pushing this evidence and tests;
root must verify successful exact reviewed-head CI before readiness. No merge,
issue closure, milestone acceptance or broad portable compatibility claim.

## Sequence iteration/reversal preparation — 2026-10-01

Isolated branch portable/core-sequence-iteration from candidate1030d74c4e prepares
complete pinned IndexedSeqIterator1644/RSeq1768 and reversible?3356/rseq3361.
No source method removal, new runtime ABI/type/bootstrap-cell or fake dependency.
Import86/artifacts90/reviews176partial+889unassessed and new53 candidates are
UNVERIFIED preparation counts only. No native/fresh/full success claim yet.
Root reads source and creates original fixtures while reviewer103 exclusively
owns heavy slot. Must rebase onto final reviewed103 parent, validate and actually
execute fresh pin/native, correct failures transparently, then full baseline and
mandatory PR review/fixes/final-head CI before readiness. No merges/closures.
No RUSTFLAGS; shared target/build2/test threads2. Hash/printing/index helpers,
generic reverse, other persistent collections/lazy/chunked/transducers and full
compiled macro/release acceptance remain unfinished.

PR102 READY, still unmerged: revieweda9f19e8650c1c2079e2c5066ebde076f9ea284a8,
exact-head CI36842775585 SUCCESS completed2026-10-01T09:42:17Z; root inspected
/private/tmp/suss-pr102-final-ci.log, updated body and ran gh pr ready102.
PR103 reviewere11771eb368a75b559a31a90a8737440a12a4a66 pushed, no significant
production finding; fresh65reduction/81control, Python80/import86/setup4/
reviews172+893/HIR1/full40341 terminal0. All reviewer handles terminal and slot
released explicitly. Final-head CI36844766535 live; PR103 stays draft.

Iteration preparationdf6dddf rebased onto reviewede11771eb; handoff append conflict
resolved preserving both complete sections. Initial fresh47858 ended101: all53
primary observations exact, native failed at named array .length write, an existing
explicit unsupported named-property adapter. Source iterator/RSeq only read length;
accepted contract has no general JS/Closure interoperability promise. Preserved
that exact source/expected object in iteration-length-boundary.json, separately
freshcertified and native required typed-error/storage/GC recovery. Other52 unchanged.
No excluded core declaration, fake native match or skipped failure.

Fresh8957 ended101:52shared+1boundary primary exact, native new provisional guard
wrongly expected Object fixed-method extras to error. Prior pinned Object semantics
ignore evaluated extras. Removed only that new error assertion, added a primary/
native operand-effects case; final26590 ended0 with53shared matches,1separate
primary boundary and2native tests. Log
/private/tmp/suss-iteration-primary53-boundary1-final.log. Initial failed logs
/private/tmp/suss-iteration-primary53.log and
/private/tmp/suss-iteration-primary52-boundary1.log retained. Corpus test now prints
case ID/source on eval errors and independently decodes each exactly-once result.
New native saved iterator/reverse views survive GC, source errors leave underlying
array intact, subsequent numeric growth/live reads and reverse fixed count execute.

Python65494 terminal0,80 pass /private/tmp/suss-iteration-python.log;
import90/setup4/reviews176partial+889unassessed/diff checks pass. Review rationale
now records actual evidence; reproducible manifest regenerated. Full60603 live,
/private/tmp/suss-iteration-full.log, root exclusively owns heavy slot. Shared
Cargo target/build2, --locked/--test-threads=2, no RUSTFLAGS. Required terminal
baseline before final commit/push/draft PR against103; mandatory independent
review/fixes/full/exact final reviewed-head CI before readiness. No merge.
Hash/printing/index helpers, generic reverse, other collections/lazy/chunked/
transducers/compiled macros and M2–M9 acceptance remain incomplete.

Full60603 terminal0; root inspected /private/tmp/suss-iteration-full.log through
final reader doc tests. Required workspace baseline passes with iteration2,
parent sequence6/137, reduction2/65, control3/81, HIR metadata1, closure14 and ABI41.
Existing manual ignored and diagnostic9passing/7knownfailures remain explicit;
no broad compatibility acceptance. All root handles47858/8957/26590/65494/60603
terminal, two failures above preserved. Ready for final candidate commit/push/draft
PR and independent reviewer with exclusive heavy-slot release. Exact final
reviewed-head CI still gates readiness; no merge or issue/milestone closure.


## Independent PR104 review — 2026-10-01

Reviewed candidate1d4cbf0379510bd551ced43914d36ad2801edace against reviewed
PR103 basee11771eb368a75b559a31a90a8737440a12a4a66 in isolated
/private/tmp/suss-review-pr104 with actual detached pinned upstreamc4295f30.
Read AGENTS/accepted design/ROADMAP/inventory/handoff. Inspected complete
IndexedSeqIterator/RSeq methods, metadata/docstrings/arities of reversible?/rseq,
hash-bound patches and EPL provenance. Mutable offsets, live storage/length,
UTF-16, cloning/tails/metadata, public protocol/Object dispatch, source equality
and Reduced stopping behavior align with the pin. No significant production
defect found in this bounded port. Hash/printing/index helpers remain pending.

Named .length writes are an explicitly unsupported host-property adapter, outside
the general JS/Closure interoperability promise. Retained source only reads that
property. The exact separate one-case boundary remains fresh certified false on
the pin and requires native Language error/storage recovery. It is not a shared
matching value, skip or reason to exclude a portable core definition. The retained
global-array negative guard separately proves failed writes preserve owned storage.

Independent original fresh61992 ended0:53 shared exact primary/native2 and the
separate1 primary boundary. Six review probes preserve every original case and add
exhausted offset advancement, UTF-16 starting offset, direct reversal dispatch,
clone storage/metadata sharing, live reverse tails and Reduced stopping indexed
reads. Fresh12322 ended0:59 shared exact primary/native2 plus unchanged1 boundary,
/private/tmp/suss-pr104-review-oracle59.log. Original53 objects, exact boundary
text and parent corpora verified unchanged. Python80 pass,
/private/tmp/suss-pr104-review-python.log; import90/setup4/reviews176partial+889
unassessed and diff-check pass. No failed provisional probes or retargeted results
in this review. Commands use shared target/build2, no RUSTFLAGS: sh
scripts/test-iteration-oracle.sh; python3 -m unittest discover -s scripts -p
"test_*.py"; core_import.py --check; sequence_provenance.py; cljs_reviews.py.

Required independent full30823 running, /private/tmp/suss-pr104-review-full.log.
Reviewer owns heavy slot until all handles terminal and final full output
inspected, then review tests/evidence commit/push. Root must require successful
exact reviewed-head CI before readiness. No merge/issue or milestone closure.
Next source hashing/printing/index helpers, generic reverse, other persistent
types, compiled macros and M2–M9 release gates remain unfinished.

Independent full30823 ended0; inspected /private/tmp/suss-pr104-review-full.log
through final reader doc tests. Required command: CARGO_TARGET_DIR=/Users/bobby/
code/github/bobby/suss/target CARGO_BUILD_JOBS=2 cargo test --workspace --locked
-- --test-threads=2. All enabled required suites pass, including iteration2/59,
parent sequence6/137, reduction2/65, control3/81, HIR if-let1, closure14 and ABI41.
Existing manual ignored tests and explicit diagnostic9passing/7knownfailures
remain unchanged. All reviewer handles61992/12322/30823 terminal; reviewer
releases heavy slot after committing/pushing review tests and evidence. Root
must verify successful exact reviewed-head CI before readiness. No merge or
issue/milestone closure; M2–M9 acceptance remains incomplete.

## Sequence indexing/search preparation — 2026-10-01

Isolated portable/core-sequence-indexing from candidate1041d4cbf0 prepares
indexed?1606/neg?3086/linear-traversal-nth1927/nth1947/-indexOf1610/-lastIndexOf1627
and original bounded neg?1201 macro lowering. All upstream algorithms preserved;
explicit fixed defn/UTF-16 in-range charAt/error/operand-once min/max adaptations,
source hashes/EPL, no ABI/type/core-cell or fake implementation. Import92/
artifacts96/reviews183partial+882unassessed and new53 candidates UNVERIFIED;
no fresh/native/full acceptance claim. Formatter dependencies remain uninitialized.

Preparation occurred while reviewer104 exclusively owned heavy slot. Reviewer
now pushedf7e0fca6565e22c6febf2b33549cadeba19710d0; no significant finding,
fresh59shared+1boundary/native2/Python80/import90/setup4/reviews176+889/full30823
terminal0, all handles terminal and slot explicitly released. Root must rebase
onto reviewed parent then execute fresh pin/native/focused/full. Final reviewed-
head CI gates104 readiness; no merge/closure. Shared target/build2/testthreads2,
no RUSTFLAGS. Next complete and verify indexing/search, then ordered/public hash,
printing, remaining collections and compiled macros toward M2–M9 acceptance.

## Indexing execution and macro fidelity — 2026-10-01

Rebased portable/core-sequence-indexing onto reviewed104 f7e0fca; preparation
commit61d2177. Fresh original53/native1 passed75758, error/GC focused native2
passed99611 output. Generated pinned core.js demonstrates helper inc/dec macro
inlining; corrected bounded original unary expansions with literal1 and existing
checked arithmetic, preserving first-class runtime functions and operand order.
Three core-cell redefinition observations added without altering original53.
Fresh10401 terminal0 certifies56 primary/native2, /private/tmp/suss-indexing-macro56.log.
Python80/import96/setup4/reviews185partial+880unassessed/diff-check pass.
Full75515 running /private/tmp/suss-indexing-full.log; root owns heavy slot.
Formatting dependencies remain uninitialized; no complete issue/milestone claim.
Next inspect full then open and independently review the indexing PR, push
significant findings and require exact reviewed-head CI. No merges.

PR103 reviewed e11771eb368a75b559a31a90a8737440a12a4a66 exact CI36844766535
success; inspected log through final doc tests, /private/tmp/suss-pr103-final-ci.log.
Marked ready for user review. PR104 f7e0fca exact CI36846726372 still running.

Full75515 ended101: two previous imported inc/dec function tests made direct
calls now correctly resolved as macros, expecting runtime arity/coercion errors.
Corrected function tests to explicit lexical function capture, retaining every
effect/arity/typed-error/recovery assertion. Added distinct macro arity guards
for inc/dec; no certified oracle observation changed or skipped. Focused rerun
and full retry required. Initial failure /private/tmp/suss-indexing-full.log.

Focused40549 terminal0: core_import17/indexing2,
/private/tmp/suss-indexing-focused-fix.log. Full52953 retry running,
/private/tmp/suss-indexing-full-retry.log. Original reviewed parent corpora and
separate boundary bytes unchanged. Root retains heavy slot until terminal.

Required full52953 ended0; inspected /private/tmp/suss-indexing-full-retry.log
through final reader doc tests. Commands: CARGO_TARGET_DIR=/Users/bobby/code/
github/bobby/suss/target CARGO_BUILD_JOBS=2 cargo test --workspace --locked
-- --test-threads=2. All required enabled suites pass, including indexing2/56,
core_import17 and reviewed parent corpora. Existing manual ignores and explicit
diagnostic9passes/7knownfailures remain unchanged. PR105 draft candidate26a7443
opened with independent reviewer review_pr105 dispatched; exact reviewed-head
CI remains mandatory. All root heavy handles terminal; release slot to reviewer
after evidence push. No merges or issue/milestone closures. Next verify reviewer
findings/CI then retained hashing/printing and remaining M2–M9 acceptance.

## Independent PR105 review — 2026-10-01

Reviewed candidate26a7443 and evidence followup9b14f4b against reviewed104
f7e0fca6565e22c6febf2b33549cadeba19710d0 in isolated /private/tmp/suss-review-pr105
with actual detached upstreamc4295f30. Read AGENTS/accepted design/ROADMAP/
inventory/handoff and inspected all complete retained indexing/search forms,
source patches/EPL/hash provenance, overloads, dispatch order, literal min/max
adaptations, UTF-16, bounded inc/dec/neg? HIR and resolver aliases/refers/exclusions/
lexical/user shadowing/phases. No significant production defect found in this
bounded port. Clarified private defn- runtime Var metadata/namespace privacy
remain unfinished despite preserved extracted source and inventory visibility;
corrected new dependency records to macro inc/dec/neg? and regenerated manifest.
Formatter dependencies still uninitialized; no full nth/core/M2–M9 acceptance.

Six appended probes preserve every original56 source/expectation object and
parent sequence/reduction/control/iteration corpora and exact separate boundary.
They prove IIndexed dispatch before sequence traversal, search count once and
stop at first match, inc/dec/neg? lexical shadowing, once-only operand effects,
throw prevents later index effects and core-neg? redefinition independence.
Native namespace guard adds core aliases/refers, explicit refer conflict,
user globals, exclusions, GC and both compiler phases/located arity rejection.

Initial fresh42082 ended1 on new provisional grouped deftype overload syntax;
pin requires repeated deftype methods, corrected only those new source candidates.
Retry54796 certifies62 primary but ended101 on new native refer harness lacking
retained runtime bindings. Loading actual core fixes harness;37532 focused native3
ended0. Final fresh92436 ended0:62 exact primary/native3,
/private/tmp/suss-pr105-review-oracle62-certified.log. Initial logs retained in
/private/tmp/suss-pr105-review-oracle62.log and
/private/tmp/suss-pr105-review-oracle62-final.log. No original expected result
changed, no skips or false matches. Python62505 ended0:80 tests pass,
/private/tmp/suss-pr105-review-python.log; import96/setup4/reviews185partial+
880unassessed/diff-check pass. No RUSTFLAGS; shared target/build2/testthreads2.

PR104 READY and still unmerged: reviewedf7e0fca exact CI36846726372 SUCCESS;
root inspected /private/tmp/suss-pr104-final-ci.log through final doc tests and
verified unchanged head before marking ready. PR105 final reviewed-head CI is
still root readiness gate after review commit/push. Required independent full92345
live, /private/tmp/suss-pr105-review-full.log. Reviewer exclusively owns heavy
slot until authoritative terminal full output/all handles inspected. No merges,
issue closures or milestone acceptance. Next retained hash/printing, remaining
persistent collections, compiled macros and surrounding M2–M9 acceptance.

Independent full92345 ended0; inspected /private/tmp/suss-pr105-review-full.log
through all final reader doc tests. Required command CARGO_TARGET_DIR=/Users/
bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2 cargo test --workspace
--locked -- --test-threads=2. All enabled required suites pass, including native
indexing3/62, core_import17 and reviewed parent corpora. Existing manual ignores
and diagnostic9passing/7knownfailures unchanged. Final Python18859 ended0 with80
passes after provenance updates, /private/tmp/suss-pr105-review-python-final.log.
All reviewer handles42082/54796/62505/37532/92436/18859/92345 terminal. Reviewer
releases exclusive heavy slot after review commit/push; root must gate readiness
on successful exact final reviewed-head CI. No merge or issue/milestone closure.

## Numeric hash boundaries — 2026-10-01

Previous goal turn made progress: PR105 opened/pushed; root56/native2/full52953
pass; reviewed103/104 exact CI success and ready, no merges. Independent105 now
pushed19d71da6670aae60d43910e97846076ed5270e4d, fresh62/native3/Python80/full92345
terminal0, no significant production defect, all handles terminal and heavy slot
released to root. Exact reviewed-head CI36849333687 live; draft stays until gate.

New isolated portable/core-hash-numeric-boundaries rebased onto reviewed105.
Original private finite/safe-integer/floor/safe-integer-remainder adapters serve
pinned public hash's numeric branch; no new GC layout/ABI/core cell. Explicit
range/nonzero guards prevent conversion/rem traps and retain numerator signed
zero. No general js-mod/public hash claim; object conversion remains incomplete.
See docs/runtime/hash-numeric-boundaries.md for provenance, scope and limitations.
Fresh81729 ended1 on new provisional floor-negative-zero expected sign; actual
pinned observation preserved, corrected only unverified expectation. Fresh24694
ended0:68 exact primary/native1. Focused8853/native2 and compiler/ABI65505/1each
terminal0. Python80/import96/setup4/reviews185+880/diff pass. New phase-test57845
compile101 on test-only PreparedFragment Debug requirement; corrected explicit
Err matching; focused84046 live. Full/review/exact-head CI pending; root heavy
slot. Commands shared target/build2/--locked/--test-threads=2, no RUSTFLAGS.
Next complete baseline, open/review numeric PR, then retain public and collection
hashing, printing, remaining persistent types/macros and M2–M9 release gates.
No issue/milestone closure or merge; alternative algorithms remain deferred #98.

Focused84046 terminal0: native3, both-phase artifact/arity checks pass;
/private/tmp/suss-hash-numeric-boundaries-focused-final-retry.log. Required
full baseline now running /private/tmp/suss-hash-numeric-boundaries-full.log;
root owns exclusive heavy slot. No full acceptance claim until terminal.

Required full73818 terminal0; inspected /private/tmp/suss-hash-numeric-boundaries-full.log
through final reader doc tests. All enabled required suites pass, including new
native3/68, numeric HIR/IR1 and direct ABI1, plus reviewed parent indexing3/62
and source corpora. Manual ignored suites and diagnostic9passes/7knownfailures
remain explicit. Required command: CARGO_TARGET_DIR=/Users/bobby/code/github/
bobby/suss/target CARGO_BUILD_JOBS=2 cargo test --workspace --locked --
--test-threads=2, no RUSTFLAGS. Draft PR106 candidatead73ca9 is open; independent
reviewer106 prepared additional probes while root held heavy slot. Root now
releases slot after evidence push; reviewer baseline and exact final-head CI
remain mandatory. No merge, issue or milestone completion claim.


## Independent PR106 review — 2026-10-01

Reviewed candidatead73ca9 and evidence followupf083348 against reviewed105
19d71da6670aae60d43910e97846076ed5270e4d in isolated /private/tmp/suss-review-pr106
with actual detached upstreamc4295f30. Read AGENTS/accepted design/ROADMAP/
inventory/handoff. Inspected original scalar adapters, noncoercing safe-integer
predicate, HIR/IR Boolean/Number types, reserved private namespace/arity/phase
behavior, additive imports and coercion/finite/range/zero guards. Exact safe i64
remainder followed by numerator sign matches the bounded source domain. No
significant production defect found. This is a public hash prerequisite, not
public/default identity hashing or general js-mod; existing limitations remain.

Seven review probes preserve all original68 source/expected objects and parent
corpus bytes. Fresh85831 terminal0:75 exact primary/native4,
/private/tmp/suss-pr106-review-oracle75.log. They add negative modulus zero signs,
large safe divisors, scalar remainder coercion, negative safe endpoints and floor
effects once. Fourth native guard proves denominator fractional/unsafe/nonfinite/
opaque/zero rejection, both operands evaluated before numeric checks, thrown first
operand prevents later effects and signed-zero GC recovery. No review provisional
failures, original expected changes or skips. Python68129 terminal0:80 tests,
/private/tmp/suss-pr106-review-python.log; import96/setup4/reviews185partial+880
unassessed/diff checks pass. Commands shared target/build2, no RUSTFLAGS.

Root full73818 ended0 before explicit release; reviewer exclusively owns heavy
slot now. Required independent full68749 live,
/private/tmp/suss-pr106-review-full.log, command CARGO_TARGET_DIR=/Users/bobby/
code/github/bobby/suss/target CARGO_BUILD_JOBS=2 cargo test --workspace --locked
-- --test-threads=2. Must inspect terminal final output, commit/push review evidence
and release slot only when all handles terminal. Exact reviewed-head CI remains
root's readiness gate. No merge/issue closure or milestone acceptance. Next
retained public/collection hashing, printing, remaining persistent types/compiled
macros and surrounding M2–M9 release gates.


Independent full68749 ended0; inspected /private/tmp/suss-pr106-review-full.log
through all final reader doc tests. Required shared-target/build2 workspace
--locked/--test-threads=2 baseline passes, including numeric boundaries4/75,
HIR/IR contracts, runtime ABI42 and unchanged reviewed parent corpora. Existing
manual ignores and explicit diagnostic9passing/7knownfailures remain unchanged.
All reviewer handles68129/85831/68749 terminal; slot released to root after
review commit/push. Exact final reviewed-head CI still gates readiness. No merge,
issue closure or milestone acceptance; public/collection hashing remains next.

## GC-owned identity and ABI transition — 2026-10-01

Previous goal turn made progress: PR106 opened/pushed, numeric75/native4/reviewer
full68749 passed at reviewed8e0f3b51a11ea0e0173849b413e1b3a91257121d. Exact
CI36851103140 success verified/head unchanged/log through final doc tests,
/private/tmp/suss-pr106-final-ci.log; marked ready. PR10519d71da exactCI36849333687
success/log finaldocs/head unchanged, /private/tmp/suss-pr105-final-ci.log; ready.
No merges. All reviewer106 handles terminal and exclusive heavy slot released.

New isolated portable/runtime-identity-hashing from reviewed1068e0f3b5 implements
owner-held UID storage for default IHash dependency. Dated design decision moves
ABI1 to2 before hash integration; same ten-type group, appended mutable UIDslots
on closures/descriptors/ordinary objects/errors, all older offsets preserved.
No global object registry, addresses, new algorithm or source compatibility
exception. Counter-only root and typed malformed/range guards; see
identity-hashing.md for source pin, fields, limits and commands.
Initial82402 private UID regression failed101; new66255 native1 pass0. ABI63405
failed101 on eight v1 fixtures; updated actual UID initialization/inspection,
manifest mismatch now tests oldABI1. ABI64950 failed101 one stale error count;
37353 all42pass0. Fresh26855 primary18exact/native101 with missing with-meta;
retained fullIFn/MetaFn/with-meta/meta with EPL patches instead of retargeting
certified source. Fresh40264 terminal0 primary18/native2. Source count96/artifacts100,
reviews189partial+876unassessed. General IFn call syntax/highest apply/function
metadata remain unfinished, apply declared uninitialized. ABI guard28207/new
comparison correction compilefailed; corrected actual API,84913 pass0. RealABI1
and falsely labeled layouts gate77421 pass0 including linking/no init effects.
New primary26 handle8773 live; root exclusive heavy slot. Focused/final full and
independent review/fixes/exact-head CI pending. No future acceptance marked.
Next finish architecture validation, public/default and collection hashing,
printing, remaining collections/macros and M2–M9 gates. Hash evaluation deferred98.

Fresh8773 terminal0:26 exact primary/native2, /private/tmp/suss-identity-hash-primary26.log;
original18 unchanged. Focused88306 terminal0: native identity3/indexing3/sequences6/
persistent-session suites pass with forced GC, typed errors and both phases.
Python80/import100/setup4/reviews189+876/diff pass. Required full baseline and
independent review/final-head CI remain pending; root owns heavy slot.

Full86115 ended101 on three closure regressions sharing a stale four-field Error
decoder (11 passed). Decoder now requires ABI2's five fields and nil data/cause/UID;
all prior message/effect/recovery checks remain. Focused34964 ended0:14 passed,
/private/tmp/suss-identity-hash-closures-retry.log. Full retry4743 is running,
/private/tmp/suss-identity-hash-full-retry.log. Root still owns the heavy slot.

Full retry4743 ended101: definition suite12pass/1fail on another stale Error count.
A broader search found the same decoder in portable_resolution; both now require
five fields and inspect nil data/cause/UID without dropping previous assertions.
Focused74630 ended0:13 definitions+11 resolution tests pass,
/private/tmp/suss-identity-hash-definition-resolution-retry.log. Full65381 is running
in /private/tmp/suss-identity-hash-full-final.log; root owns the heavy slot.
Draft PR review may proceed statically while the baseline runs; execution must
wait for root to release the slot. Readiness still requires reviewer baseline
and exact final reviewed-head CI; no merge or acceptance closure.

Full65381 terminal0 through final doc tests at implementation262b2d7, including
all44 ABI tests and real old-layout/UID-corruption guards,
/private/tmp/suss-identity-hash-full-final.log. Ignored/manual tests remain explicit.
Draft PR107 https://github.com/bobby/suss/pull/107 opened with Refs links, base
reviewed1068e0f3b5. Independent /root/review_pr107 created isolated actual-pin
worktree and performed static review while root owned the heavy slot. Root's
full/push/create handles are now terminal; heavy slot releases to reviewer after
this evidence commit. Initial CI36856978539 is in progress at262b2d7; final reviewed
head CI still gates readiness. No merge, issue closure or milestone acceptance.
Next finish review/fixes and source public/default/ordered/unordered hashing,
general IFn invocation/apply, printing and remaining M2–M9 acceptance gates.

## Independent PR107 review — 2026-10-01

Reviewed262b2d7/root evidence160ccdc against reviewed1068e0f3b5 in isolated
/private/tmp/suss-review-pr107 with actual detached upstreamc4295f30. Read accepted
design/ROADMAP/inventory/handoff. Owner-held UID slots, all constructors, scalar
allocator, corruption guards, genuine ABI1 rejection and retained metadata source
provenance inspected. Significant finding: canonical IFn nominal direct dispatch
used ordinary protocol receiver arity, conflicting with pinned emitted IFn method
formals. Fix carries a next-arity key and prepends the already evaluated target;
ordinary protocols/native fallback/general strict invocation remain separate.
Typed malformed next-key constructor guard added. See identity-hashing.md.

Corrected dispatch exposes unchanged original metafn-explicit-invoke as an exact
strict-arity design-contract divergence: pin true, native typed Error with raw
message Wrong arity. Preserve its exact source/reference and assert it separately,
never skip/count success. Original26 now25 matching+1 boundary; all original JSON
objects and parent corpus bytes unchanged. Eight review probes bring fresh34
primary observations to33 matching native relations+1 boundary across4 tests.
Effect order, thrown receiver short-circuit, metadata/copies, GC/UID recovery and
unrelated user IFn/-invoke protocol guard pass. Highest apply/general callable
object/full metadata remain unfinished; no generic JS arity-normalizing intrinsic.

Initial fresh68141 ended1 on new provisional receiver expectations; native95912
confirmed mismatch101. New provisional callbacks corrected to accept actual pin
arguments, originals unchanged. Fix51693 compile101 private accessor;26689
validation101 missing ARGS cast; corrected both. Fresh31023/4452 primary certifies
but native101 on unchanged zero-invoke strict boundary, now recorded explicitly.
Native93996 pass4 and finalfresh29279 terminal0:34 exact primary/native4 with
33matches+1 exact boundary, /private/tmp/suss-pr107-review-oracle34-final.log.
UID exhaustion fixture92068 compile101 test-only u64 range indexing; retry61754
pass0. ABI46577 terminal0 all45, /private/tmp/suss-pr107-review-abi45.log;
final strengthened next-key fixture also runs in the full baseline. Python90053
terminal0/80, /private/tmp/suss-pr107-review-python.log. Import100/setup4/
reviews189partial+876unassessed/diff pass, no source methods removed.

Required reviewer full90116 live, /private/tmp/suss-pr107-review-full.log:
CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2
cargo test --workspace --locked -- --test-threads=2; no RUSTFLAGS. Reviewer retains
exclusive heavy slot until authoritative terminal full/all handles inspected.
Push significant fixes/evidence, then root must require successful exact final
reviewed-head CI. No merges/issue closure/milestone acceptance. Next retained
public/default/collection hashing, printing, remaining persistent types/compiled
macros and complete M2–M9 acceptance gates.

Independent full90116 ended0; inspected /private/tmp/suss-pr107-review-full.log
through final reader doc tests. All enabled required suites pass, including
identity4/33matches+1 exact boundary, ABI45 with final valid-old-key/invalid-next-key
guard and UID exhaustion/cached-owner fixture, unchanged reviewed parent corpora.
Existing manual ignores and diagnostic9passes/7knownfailures unchanged. Source
receiver provenance: pinned core.cljc1509–1540/1587. Final Python40584 ended0/80,
/private/tmp/suss-pr107-review-python-final.log. All reviewer handles92068/61754/
68141/90053/95912/51693/26689/31023/4452/93996/29279/46577/40584/90116 terminal.
Reviewer releases heavy slot after fixes/evidence push; exact final reviewed-head
CI remains root's readiness gate. No merge/issue or milestone closure. Next
retained public/default/collection hash, printing, remaining types/compiled macros
and surrounding M2–M9 release acceptance remain unfinished.


## Expanded PR95 independent CI review — 2026-10-01

The user merged PR96–107 into PR95's branch, advancing its head to
015767ae44b5e2739782fb979f6324c46a3db9eb. Its complete Git tree is identical to
independently reviewed PR107 head7b5307849aa641f0fabe26f5257335526f48fa3b
(`git diff 7b5307849aa641f0fabe26f5257335526f48fa3b HEAD --stat` is empty
before this CI change). Prior PR95 evidence atd49c5a72 certifies only the
old head, not this expanded branch.

Independent /root/review_pr95_expanded inspected run36871799306 at015767ae
and check-run110401145846 annotations. GitHub explicitly reports
“The job has exceeded the maximum execution time of 25m0s”. Inventory/scanner
verification passed. Workspace compilation and preceding enabled suites passed;
conformance was still executing when the job was cancelled. This is not a passing
full baseline. Authoritative log: /private/tmp/suss-pr95-expanded-ci.log.

Change only the job timeout from25 to35 minutes, matching the separately reviewed
PR108 CI allowance. Commands, enabled suites, worker count and concurrency are
unchanged. No production or test source changes; no local cargo/JVM/Node runs
while root owns the heavy slot. Previous PR107 full90116 and exact-head CI
36859350775 remain prior evidence, not new-head CI certification. Require the
new reviewed commit's successful complete CI before PR95 readiness; no merge or
issue/milestone closure. Next continue retained collection/vector dependencies
and remaining M2–M9 acceptance gates.

## Default IHash source integration — 2026-10-01

Prior goal turn made progress: PR107 implementation and full-baseline evidence
committed/pushed at160ccdc; independent reviewer dispatched, no merges. Review
now found an IFn receiver defect; fixing it reveals the original zero-argument
MetaFn explicit invocation as a strict wrong-arity design boundary. Preserve its
primary observation and assert native error separately; no JS argument-padding
adapter is authorized. Reviewer retains heavy slot for fixes and certification.

Root isolated portable/core-default-identity-hash from160ccdc prepares the next
source dependency while review runs. Added zero-arity private adapter to existing
canonical Object prototype, whole retained root-obj and default IHash setup,
explicit source/patch hashes and stricter standalone patch provenance. Generated
selection97/artifacts101/reviews190partial875unassessed. Static import/provenance
and Python81 pass;18 new observations are PROVISIONAL, native tests unexecuted.
No cargo/JVM/Node runs overlap reviewer. Rebase on final reviewed PR107 before
executing fresh oracle/native/focused/full and opening its separately reviewed
PR. Public hash retains its Date/case dependencies; do not remove branches to
make a smaller function pass. No issue or milestone acceptance claimed.

Review107 fresh29279 terminal0:34 primary observations/native4 pass, including
33 matching+1 preserved exact zero-invoke strict boundary. Canonical IFn receiver
fix keeps unrelated user IFn protocols ordinary. ABI45 focused46577 is live;
reviewer retains heavy slot before mandatory full. Root default-IHash corpus now
21 provisional cases, adding direct method priority/captured dispatcher/live root
and root UID independence; native and oracle remain unexecuted.

Review107 authoritative full90116 terminal0/all handles terminal; significant
IFn fix pushed7b5307849aa641f0fabe26f5257335526f48fa3b, slot released to root.
Exact CI36859350775 running at reviewedhead; draft/readiness still gated. Root
updated PR107 description/title to final receiver fix and34primary/33matches+1
strict boundary, no merge. Default branch rebased onto7b530784, preserving both
handoff sections and regenerating conflicting manifest (source unchanged).
Initial native57181 terminal0/2; fresh24294 terminal0/21exact+native2, expected
development private-var warnings preserved: /private/tmp/suss-default-hash-primary21.log.
Compiler13236 terminal0/1, /private/tmp/suss-default-hash-hir-ir.log. Parent focused
15708 terminal0 (core interfaces, identity4/strict boundary, sequences6),
/private/tmp/suss-default-hash-parent-focused.log. Python81/import101/setup5/
reviews190partial875unassessed/diff pass. Full/review/final-head CI still required.
Root owns the heavy slot for the next full baseline. Public hash's complete
Date/case branches, ordered/unordered composition and remaining gates are next.

Required default-IHash full82236 terminal0 through final reader doc tests,
/private/tmp/suss-default-hash-full.log, including all45 ABI regressions and
unchanged parent oracle/native boundaries. Draft PR108 opened atbefb2c0 with Refs
links, base reviewed1077b530784. Mandatory /root/review_pr108 dispatched in
isolated actual pin; static review found no significant defect so far and seven
new probes are provisional pending certification. Root's cargo/oracle/push/create
handles57181/24294/13236/15708/82236/59134/72327 terminal. Heavy slot releases to
reviewer after this evidence push. Exact reviewed-head CI and review fixes/full
still gate readiness. No merges/issue closures/milestone acceptance.


## Independent PR108 review — 2026-10-01

Reviewedbefb2c0/root evidence9bb3f92 against independently reviewed1077b530784
in isolated /private/tmp/suss-review-pr108 with actual detached upstreamc4295f30.
Read accepted design/ROADMAP/inventory/handoff. Complete root/default IHash
source, live root lookup, existing canonical prototype storage, zero-arity
HIR/IR adapter, standalone hash-bound patch provenance and EPL artifacts
inspected; no significant production defect found.

Seven additive probes preserve all original21 source/reference objects and
parent corpora. Fresh19855 terminal0:28 exact primary/native2,
/private/tmp/suss-pr108-review-oracle28.log. They cover direct priority without
root lookup, default root once, target-before-root effects, root restoration,
throw recovery and zero-root hash not allocating a UID. Native67088 terminal0:
3 tests, /private/tmp/suss-pr108-review-native3.log; extra guard preserves
captured dispatcher/owner/provider across forced GC and live root redefinition,
restores original root and UID, and rejects a thrown target before root effects.
Compiler35764 terminal0/1, /private/tmp/suss-pr108-review-compiler.log.
Python48498 terminal0/81, /private/tmp/suss-pr108-review-python.log; import101/
setup5/diff checks pass. No provisional failures, skipped results or changed
original expectations. PR107's exact IFn strict-arity boundary remains unchanged.

Root full82236 terminal0/all handles terminal before explicit slot release.
Reviewer required full78254 is live /private/tmp/suss-pr108-review-full.log,
CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2
cargo test --workspace --locked -- --test-threads=2; no RUSTFLAGS. Reviewer
retains exclusive heavy slot until full/all handles terminal, then commit/push
review evidence. Exact reviewed-head CI gates readiness; no merges, issue
closure or milestone acceptance. Next complete public hash Date/case branches,
ordered/unordered composition, remaining collections/macros and M2–M9 gates.


Independent full78254 terminal0; inspected /private/tmp/suss-pr108-review-full.log
through final reader doc tests. Required shared-target/build2 workspace --locked/
--test-threads=2 baseline passes, including default3/28, ABI45, all unchanged
reviewed parent corpora and exact IFn strict-arity boundary. Existing manual
ignores and diagnostic9passes/7knownfailures remain unchanged. All reviewer
handles19855/48498/67088/35764/78254 terminal. Reviewer releases heavy slot after
review evidence push; root must gate readiness on successful exact reviewed-head
CI. No merge, issue closure or milestone acceptance.


### PR108 final-head CI timeout correction — 2026-10-01

Exact reviewed707b5b609fa139ee033f62f4424ca8b78c6b49c4 run36861360401
attempts1 and2 are authoritative completed CANCELLED, not passing CI.
Attempt1 required workspace step reported success after24m29s, but the overall
job elapsed25m03s and was cancelled. Attempt2 job elapsed25m15s and cancelled
the required step after24m39s. The25-minute job budget leaves insufficient
room for checkout/toolchain/provenance and cleanup. Independent followup raises
only timeout-minutes25→35 with the same explanatory comment already reviewed
in PR110f7841dd. No checks, worker limits or test commands removed or reduced.

Production, runtime artifacts, compiler/native/Python tests and oracle corpus
bytes are unchanged from707b5b6, verified by scoped Git diff. Thus prior reviewer
full78254 remains applicable to the identical tested code, but it is not a fresh
run at this followup commit. Root owns the heavy slot (publichash full61362);
no cargo/JVM/Node process was started for this static workflow/evidence fix.
Fresh exact final-head CI is mandatory; draft remains, no readiness/merge claim.


### PR108 retargeted stack rebase — 2026-10-01

User reported merging a large stack as#109. Root GitHub audit found no#109
and found#96–107 merged into the stack branch while#93–95 remain open and main
still ends at#91. Reviewer independently verified#107 MERGED at13:51:09Z and
#108 OPEN with auto-retargeted base portable/core-bitwise-hash/CONFLICTING.
Fetched base015767ae44b5e2739782fb979f6324c46a3db9eb has an exactly identical
tree to reviewed1077b5307849aa641f0fabe26f5257335526f48fa3b. Rebased only the
five PR108 commits with --onto015767ae 7b530784; no conflicts. Entire rebased
tree was byte-identical to reviewedc6cc97b before this evidence append.
Scoped diff proves all production/tests/oracles/runtime/workflow/lock bytes remain
identical after this append, so prior local baseline applies to unchanged code,
not a newly executed full at this commit. No heavy processes started; root keeps
the exclusive slot. Push uses exact old-head force-with-lease. Fresh final-head
CI remains required; no readiness/merge/issue or milestone closure claim.


### PR108 full-test success cancelled during cache cleanup — 2026-10-01

Exact23fe2528782d28e60901621fe7dd1e805ae91a78 CI36872972201 completed CANCELLED.
Independent annotation inspection confirms: "The job has exceeded the maximum
execution time of 35m0s". Setup/provenance succeeded in29s; required workspace
step succeeded14:01:55–14:36:33 (34m38s), including final reader doc tests.
Post rust-cache14:36:33–14:36:39 was cancelled while tar/zstd cleanup remained;
job elapsed35m17s. Full log /private/tmp/suss-pr108-ci35-cancelled.log preserved.
This is a job-budget failure, not a passing final-head run or a hidden test error.

Independent correction increases only job timeout35→60 minutes, allowing
substantial runner/cache variance after two observed full-step durations24m29s
and34m38s. Retains every enabled check, locked command, two build workers and
two test threads; no production/test/runtime/oracle/lock bytes changed relative
reviewed23fe252. Prior local full78254 still covers identical tested code, not a
fresh local execution for this static fix. No cargo/JVM/Node process started;
root retains exclusive heavy slot/full22313. Descendants are not rebased here.
Fresh exact-head CI mandatory before readiness; no merge or milestone claim.


### PR108 rebase onto reviewed PR95 CI fix — 2026-10-01

Rebased only PR108 commits from oldbase015767ae onto independently reviewed
PR95head447bed49003938ea69fd5ff752b69a92330c9ba2. Preserved its complete
26-line expanded-baseline review evidence and every PR108 evidence section.
Resolved workflow conflict to exact reviewed7ffad606 bytes: final budget60;
PR95ancestor retains its own35-minute change. Scoped diff proves crates/scripts/
runtime/tests/oracles/workflow/Cargo/lock bytes identical to7ffad606 after rebase.
Only handoff gains PR95 evidence and this record; no fresh local full claimed.
No heavy runs or descendant changes. Push uses exact7ffad606 force-with-lease;
fresh reviewed-head CI remains required before readiness, no merge claim.


## Scalar case bootstrap — 2026-10-01

Branch portable/core-scalar-case, isolated /private/tmp/suss-core-scalar-case,
based on independently reviewed PR108707b5b609fa139ee033f62f4424ca8b78c6b49c4.
Actual detached upstream c4295f30. Original Rust bootstrap follows pinned
case/assoc-test provenance and EPL1 notice; selector once, source-order analysis,
private number/string comparison, live qualified constant-first core equality,
explicit defaults, bounded scalar groups and phase namespace resolution.
No full compiled macro acceptance or issue/milestone completion claim.

Initial regression41004 failed101 on unresolved Runtime case; implementation
3264 passed1. Fresh88591 failed1 on pinned empty-group invalid generated JS.
The unchanged source now has a separate strict reference failure/native located
compile-error boundary, not a successful value or skip. Fresh63374 terminal0:
31 matches/1 boundary/native3. Fresh17662 terminal0:33 matches/1 boundary/native4,
/private/tmp/suss-case-primary33-boundary1.log. Compiler95117 terminal0/1 actual
validated Runtime/Macro execution and forced GC,
/private/tmp/suss-case-both-phases.log. Full compiled upstream macros remain open.

A strengthened direct boundary invocation initially failed because Node resolved
Closure output relative to the repository root. Captured MODULE_NOT_FOUND did
not count as the expected failure. Explicit oracle cwd fixes the harness;
root invocation now verifies exit1, empty stdout, exact SyntaxError and source
caret from the original generated input. Python82 pass in
/private/tmp/suss-case-python82-final.log. Import101/reviews192+873/setup5/diff
checks pass. Parent focused22322 and its first retry failed101 on mistaken test
target names before running tests; corrected90679 terminal0 passes control3,
default3 and identity4, /private/tmp/suss-case-parent-focused.log.

Required full16020 running /private/tmp/suss-case-full.log, shared target/build2,
cargo test --workspace --locked -- --test-threads=2, no RUSTFLAGS. Root retains
exclusive heavy slot until authoritative terminal result. Independent PR review
and exact reviewed-head CI remain required before readiness.

PR1077b530784 exact CI36859350775 succeeded; final reader doc tests inspected.
PR107 is ready, no merge. PR108707b5b6 independent reviewer full78254 terminal0;
exact CI36861360401 completed cancelled near its25-minute job limit despite the
workspace step reporting success. It does not satisfy the overall CI gate;
rerun dispatched at unchanged head, PR108 remains draft. No merge or closure.
Next: complete this full/review/CI gate, then public hash Date branch and ordered/
unordered composition, remaining macro/collection/core and M2–M9 acceptance.


PR110 opened draft at5ec1492; mandatory /root/review_pr110 dispatched for static
review while root full16020 owns heavy slot. PR108 CI attempt2 is live at the
same707b5b6. PR108's preceding required workspace step took24m29s, leaving no
reliable cleanup margin under the25-minute job timeout. PR110 raises the job
limit to35 minutes without changing the full command, enabled tests or build
concurrency; independent review and exact-head CI must validate this change too.


Root full16020 terminal0; /private/tmp/suss-case-full.log inspected through final
reader doc tests. Required workspace --locked/--test-threads=2 shared target/
build2 baseline passes with new native4/compiler1, existing ABI45 and unchanged
parent corpora/strict-arity boundary. Existing manual ignores and diagnostic
known failures remain explicit. All root handles22322/90679/16020/40683/50092
terminal; mistaken-target retries also terminal101. Release exclusive heavy slot
to /root/review_pr110 after this evidence push. Independent oracle/full and exact
reviewed-head CI still gate PR110 readiness; PR108 attempt2 pending. No merges.


## Independent PR110 review — 2026-10-01

Reviewed5ec1492 plus root CI/evidence commits throughd12f061 against reviewed
PR108707b5b6 in isolated /private/tmp/suss-review-pr110, actual detached upstream
c4295f30. Read accepted design/ROADMAP/inventory/handoff and pinned case/assoc-test.
No significant production defect found. Mixed number/string private comparison,
generic qualified constant-first equality, bounded insertion order, selector
once, tail/source analysis, aliases and strict empty-group negative boundary
inspected. CI timeout25→35 responds to actual cleanup cancellation, retaining
all required suites, shared engines and two workers.

Root full16020 terminal0/all handles terminal before explicit heavy-slot release.
Seven additive observations preserve original33 source/reference objects and all
parent corpora. Fresh81349 terminal0 certifies40 exact matches plus1 original
exact parse boundary/native4, /private/tmp/suss-pr110-review-oracle40.log.
Native76696 terminal0/5 adds thrown equality restoration, no unselected effects,
GC retained closure, signed-zero duplicate and ninth generic constant/atomic
publication guards: /private/tmp/suss-pr110-review-native5.log. Compiler41542
terminal0/1 actual Runtime/Macro execution, /private/tmp/suss-pr110-review-phases.log.
Python82 pass /private/tmp/suss-pr110-review-python82.log; import101/reviews192+873/
setup5/diff pass. Mistaken nonexistent core_setup_provenance.py failed; actual
sequence_provenance.py corrected/verifies5. No skips, altered expectations or
claimed compiled-macro acceptance. Reviewer owns heavy slot for required full.
No merges/issue closure/milestone acceptance. Public hash Date branch, ordered/
unordered composition, complete collections/macros and M2–M9 release gates next.


Independent full69411 authoritative terminal0; inspected
/private/tmp/suss-pr110-review-full.log through final suss_reader doc tests.
Required shared-target/build2 workspace --locked/--test-threads=2 baseline passes,
including case5/40 plus exact negative boundary, both phase fragments, ABI45 and
unchanged parent corpora/boundaries. Existing manual ignores and diagnostic
9passes/7knownfailures unchanged. All reviewer handles81349/76696/41542/69411
terminal. Reviewer releases heavy slot after evidence commit/push. Exact final
reviewed-head CI still gates readiness; no merge/issue or milestone closure.

### PR110 reviewed-stack rebase — 2026-10-01

Static rebase of reviewed f7841ddcae14d5094499148438eca1decf50619e onto
reviewed PR108 c6cc97b134555c42104310156198c3f0a1a2b2c5 preserves parent timeout
and child scalar-case/full-review evidence. Scoped Git diff verifies all crates,
runtime artifacts, scripts, tests/oracle corpora, Cargo files and CI workflow
bytes identical to f7841dd. The only inherited differences are parent handoff
and default-identity-hash documentation. Prior independent full69411 applies to
identical tested code; no fresh baseline at this rebase is claimed. Root owns
the heavy slot with publichash full61362; no cargo/JVM/Node process started.
Exact new reviewed-head CI remains mandatory before readiness; no merge/closure.

### PR110 parent retarget reconciliation — 2026-10-01

Statically rebased reviewed d507c5ecb2afa5c9319fcbdef3112c99fd699d4e onto
reviewed PR10823fe2528782d28e60901621fe7dd1e805ae91a78 after user stack merges.
Preserved parent retarget audit and all child case/review/full evidence in the
append conflict. Scoped Git diff verifies production crates, runtime artifacts,
scripts, tests/oracle corpora, workflow and Cargo bytes identical to d507c5e.
Only parent audit and this reconciliation evidence change. Prior full69411
applies to identical tested code; this is not a fresh baseline. Root owns the
heavy slot for collection oracle1484; no cargo/JVM/Node process started here.
Exact old-head force-with-lease protects the owned branch; successful exact new
reviewed-head CI remains required, with no readiness/merge/closure claim.

### PR110 historical CI success and updated parent budget — 2026-10-01

Independent verification confirms exact c53cfe35fde0310a8ceacf174d399ce48fd1e646
CI36873277022 completed SUCCESS. Downloaded log
/private/tmp/suss-pr110-c53-final-ci.log inspected through final reader doc tests.
Rebased only reviewed case commits onto reviewed PR108e2d8b6123b7948a0210e5330c5881894d47ce480,
preserving parent CI/audit evidence and every child review/test/evidence section.
Scoped Git diff proves crates/runtime/scripts/tests/oracle/Cargo bytes identical
to c53cfe3. Workflow exactly matches parent: budget60, every test/worker unchanged.
This inherits the independently audited35-minute cleanup cancellation correction.
Prior independent full69411 covers unchanged tested bytes, not a fresh baseline.
Reviewer113 owns the heavy slot; no cargo/JVM/Node started. Historical successful
CI does not satisfy the new exact-head gate. No readiness/merge/closure claim.

## Retained public scalar hash — 2026-10-01

Isolated /private/tmp/suss-core-public-scalar-hash branch
portable/core-public-scalar-hash, based on independently reviewed PR110f7841dd.
Actual detached upstream c4295f30, checkout19903 terminal0. Root regained heavy
slot after reviewer110 full69411 terminal0 and terminal review pushf7841dd.
PR11040 fresh matches/native5/compiler1/Python82/full all pass; exact reviewed
head CI36867771937 remains live; no readiness/merge/closure yet.

Public hash regression30006 terminal101 failed unresolved Runtime hash. Complete
source extraction1054–1090 and EPL notice now retain all branches, numeric guards,
case, strings, direct IHash priority, nil/default and Date. Explicit patch uses
existing private numeric guards and original BootstrapDate nominal storage;
checked numeric TimeClip, ordinary Object valueOf method and existing shared
ABI2. No global registry, ABI layout change or shipped Java/Node. General Date
parsing/mutation, reader instants, Inst/equality/comparison/printing and bootstrap
privacy/Var metadata remain unfinished. Internal helper private flags initially
failed compile75868; removed unsupported attributes with limitation recorded.
43270 and localized90709/32208 failed validation due missing unary emitter import
signature for new TimeClip. Corrected import;59391 terminal0/native3. Date49538
terminal0/native3. Temporary redundant core-split test removed after localization;
standalone numeric Date regression retained. No ignored/masked validation error.

Fresh83231 terminal0:42 exact pinned/native observations/native4,
/private/tmp/suss-public-hash-primary42.log. Provisional expectations were only
certified after fresh pinned output and independent raw f64/i31 decoding matched.
Date fixture uses explicit reader-conditional native/host adapter; expected pin
root-obj private-var warning is recorded. New corpus includes signed zeros,
modulus/fraction/subnormal/non-safe/Inf/NaN, UTF16, protocol/direct/default/root,
captured/qualified calls, effects and clipped/invalid numeric Date branches.
Native Date probes also validate GC, valueOf redefinition, IHash priority and
typed unsupported-input recovery. Python82, import102/reviews193+872/setup5/diff
pass. Parent47829 terminal0: case5/default3/identity4,
/private/tmp/suss-public-hash-parent-focused.log. Full61362 LIVE,
/private/tmp/suss-public-hash-full.log, CARGO_TARGET_DIR shared target,
CARGO_BUILD_JOBS=2 cargo test --workspace --locked -- --test-threads=2, noRUSTFLAGS.
Root owns heavy slot until authoritative terminal; all other root handles terminal.

PR108 CI36861360401 attempt2 also CANCELLED near25m (not a pass). Reviewer108
has a static follow-up to apply25→35 timeout without production/test changes,
prove bytes unchanged, push and require new final-head CI; cannot run heavy jobs
while root61362 live. PR108 stays draft. No issue/milestone completion from this
slice. Next finish full/review/CI, then ordered/unordered hashing and full remaining
Date/collection/core/macro and M2–M9 acceptance. Issue98 still defers algorithms.


Root publichash full61362 authoritative terminal0; inspected
/private/tmp/suss-public-hash-full.log through final reader doc tests. Required
shared target/build2 --workspace --locked -- --test-threads=2 passes newnative4,
ABI45 and unchanged parentcorpora/strict-arity/referenceparse boundaries. Existing
manual ignores and diagnostic known failures remain explicit. All root handles
19903/30006/75868/43270/90709/32208/59391/49538/83231/47829/61362/69372 terminal.
PR111 draft2d94d7b opened, mandatory reviewer111 staticreview dispatched.

Reviewer108 timeoutfix c6cc97b134555c42104310156198c3f0a1a2b2c5 has CI36869717334
live; production/test bytes identical707 baseline. Attempt1 fullstep reported
success24m29, attempt2 fullstep cancelled24m39; neither overallcancelled run is
CIpass. Reviewer110 staticrebase of reviewedf784 onto108c6 is underway toresolve
stackdoc conflicts; root111 will rebase afterward preserving code and evidence.
No heavy processes remain at root; reviewer111 receives exclusive slot after
this evidence push and finalroot rebasing. Exactreviewedhead CI still gates all
readiness. No merges, issue closures or milestone acceptance.


PR111 stack rebase onto independently reviewed PR110d507c5e complete. Handoff
append conflict resolved by retaining parent PR110 rebase evidence and the entire
child publichash evidence. git diff --exit-code 7eca624 -- crates runtime scripts
tests Cargo.toml Cargo.lock .github/workflows/ci.yml proves byte-identical tested
production/tests/oracles/workflow; no fresh baseline is claimed for documentation
and ancestry changes. Root full61362 applies to these same bytes. Release exclusive
heavy slot to reviewer111 after this evidence push; all root test/push handles
terminal. Independent review/probes/final full and exact reviewed-head CI remain
required. PR108c6 and PR110d507 CI pending; no merges or milestone/issue closures.


## Independent PR111 review

Reviewer /private/tmp/suss-review-pr111 has actual detached upstreamc4295f30,
no symlinks, and moved probes onto rebased head1c99e741. Initial git move failed
sandbox index permission; authorized escalation succeeded. Oracle87183 started
during that move; rebased production/scripts/source bytes were identical and
checkout completed before native compilation. No source probe was altered.
Original42 corpus objects preserved exactly; five new cases were provisional
until fresh pinned observations matched. Oracle87183 terminal0:47 exact/native5,
/private/tmp/suss-pr111-review-oracle47.log. Added safe integer endpoints,
captured hash/live string helpers, direct protocol throw effects and negative
submillisecond Date. Added raw native TimeClip type guards for nil/bool/string/
closure, effects surviving language errors, zero normalization, throw recovery,
invalid unary/first-class compile isolation. Compiler25619 terminal0/3 includes
TimeClip malformed HIR/IR arity/result checks. Python42267 terminal0:82 pass,
import102/setup5/reviews193partial+872unassessed/diff pass. No production finding.
Required independent full27630 LIVE /private/tmp/suss-pr111-review-full.log,
shared target/build2 --workspace --locked -- --test-threads=2; noRUSTFLAGS.
Reviewer retains exclusive heavy slot until authoritative terminal and release.
No merge/readiness/issue or milestone closure. Exact reviewed-head CI required.


Reviewer full27630 authoritative terminal0; inspected final suss_reader doc tests
and no failed-test/error entries in /private/tmp/suss-pr111-review-full.log.
Shared-target/build2 required workspace --locked/--test-threads=2 passes including
native5/47 exact observations, TimeClip compiler guards and unchanged parents.
Existing explicit manual ignores remain. All reviewer handles87183/25619/42267/
27630 terminal. Reviewer commits and pushes probes/evidence, then releases heavy
slot. Exact final reviewed-head CI still gates readiness; no merge or closure.


### PR111 parent retarget reconciliation — 2026-10-01

Static rebase of independently reviewed9f3c19cad8f5f344f077a54a469532ca716352a8
onto reviewed PR110c53cfe35fde0310a8ceacf174d399ce48fd1e646 preserves parent
retarget audits and all public-hash child/review evidence. Append-only handoff
conflict preserved both sides. Scoped Git diff verifies all crates, runtime,
scripts, tests/oracle, workflow and Cargo bytes identical to reviewed9f3c19c;
only inherited parent audit and reconciliation evidence change. Prior independent
full27630 applies to identical tested code; no new baseline is claimed. Root
owns heavy slot full collection6189; no cargo/JVM/Node process started here.
Exact old-head force-with-lease protects the owned branch. New reviewed-head CI
remains mandatory before readiness; no merge, issue or milestone closure.


### PR111 inherited timeout reconciliation — 2026-10-01

Prior exact-head CI36873588930 completed SUCCESS at revieweddec9e684; downloaded
/private/tmp/suss-pr111-dec9-ci.log and inspected through final suss_reader doc
tests. This is historical success, not new-head certification. Statically rebased
onto independently reviewed PR11007f0a97b20d918cb2b4589831f3ebdf6298e958c,
preserving all parent timeout audits and child review evidence. Scoped Git diff
proves crates/runtime/scripts/tests/oracle/Cargo bytes identical to revieweddec.
Inherited workflow35→60 and explanatory comment match the reviewed parent; no
suite, worker, lock or fuel gate changed. Prior full27630 remains identical-code
evidence; no fresh cargo/JVM/Node started while reviewer113 owns heavy slot.
Exact new reviewed-head CI remains mandatory; no readiness/merge/closure.
## Retained collection hashing — 2026-10-01

Isolated /private/tmp/suss-core-collection-hashing branch
portable/core-collection-hashing based on independently reviewed1119f3c19c;
actual detached upstreamc4295f30, checkout31030 terminal0. Reviewer111 fresh87183
terminal0/47/native5, compiler25619/3, Python/provenance42267, full27630 and
push29074 allterminal0. No significant defect; root regainedexclusiveheavy slot.

Initial33112 terminal101: unordered unresolved compile diagnostic, sequential
hash Language exception before declared ordered helper initialization. Complete
pinned helpers1406/1420 and empty unordered initializer1432 now have explicit
fixed-defn/private-metadata bootstrap patches retaining source algorithms,
metadata/docstrings and EPL provenance. No new native primitive or shared ABI
change. Runtime Var privacy remains unfinished. Import101 selections/105 artifacts;
196 partialreviews/869 unassessed. First89014 terminal0/native2.
Fresh8437 terminal0/44/native4 /private/tmp/suss-collection-hash-primary44.log.
Strengthened71740 terminal0: raw thrown Number17/exact Error Wrong arity guards,
collection4/public5/default3/identity4 parents, Python82. Additional five cases
preserve first44 objects; fresh1484 terminal0/49/native4,
/private/tmp/suss-collection-hash-primary49.log. Effects encode exact source order;
unordered output ignores order while traversal effects retain it. Duplicate count,
UTF16, nested/metadata/cached/reverse sequence and empty initializer values match
fresh pin and independently decoded raw f64/i31 storage. Provisional numbers
were not called certified until both actual executions passed. Expected pin
private-var/trace-redefinition warnings recorded. Import/reviews/setup5/diffpass.

Required full6189 LIVE /private/tmp/suss-collection-hash-full.log,
CARGO_TARGET_DIR shared target/CARGO_BUILD_JOBS=2 cargo test --workspace --locked
-- --test-threads=2, no RUSTFLAGS. All other root heavy handles terminal; root
retains exclusive slot until authoritative full completion. Independent PRreview
and exact reviewedheadCI will gate readiness. No issue/milestone closure.

User reports merged stack #109. GitHub cannot resolve109 as issue/PR. Verified
#96–107 merged2026-10-01T13:50–13:51 into stack branch95; #93–95 OPEN, main at
merged91 89e5dce. origin/portable/core-bitwise-hash now015767ae (merge107), entire
Git tree identicalreviewed1077b530784. GitHub auto-retarget108base to95 andmarked
CONFLICTING. Reviewer108 staticrebase reviewedc6 onto0157 yielded23fe252; all
production/tests/workflow bytes unchanged, nowMERGEABLE. Reviewer110 staticrebase
d507 onto23fe yieldedc53cfe35, bytesunchanged; reviewer111 staticrebase9f3 onto
c53 underway. No cargo/JVM/Node from those followups; rootowns6189 slot.
Current stale CI heads are not readiness evidence; require each exactnew reviewed
head CI. Rootcollection willrebase on111newhead once available, preserving source
and evidence. No rootmerge authorized/performed. Next full/review/CI, then actual
persistent vectors/maps/sets/map entries and remaining core/macro/release gates.


Root collection full6189 authoritative terminal0 through final reader doc tests,
/private/tmp/suss-collection-hash-full.log. Required shared-target/build2 workspace
--locked/--test-threads=2 passes collection4, parentpublic5/47, identity4/exact
strict-arity boundary, case5/40/exactparseboundary, ABI45 and unchanged enabled
parent suites. Existing manual ignores and diagnostic9passes/7knownfailures remain
explicit. All root handles31030/33112/89014/8437/71740/1484/6189/59060 terminal.
Reviewer111 staticrebase nowdec9e684 onto110c53/10823fe; production/test/workflow
bytes identical9f3. Remaining108/110/111 MERGEABLE; exactcurrentCI pending.
PR95mergedstackhead015767ae has CI36871799306 pending; do not use oldd49 readiness
checks for this expanded head. Root will rebase collection changes on111dec9e684,
prove production/test inputs unchanged, then dispatch mandatory independent review
with exclusive heavy slot. No rootmerge, issue or milestone closure.


Collection branch rebase onto independently reviewed111dec9e684 complete.
Append-only handoff conflict preserved parent retarget audit and child hashing
source/test/full evidence. git diff --exit-code 8bd6e18 -- crates runtime scripts
tests Cargo.toml Cargo.lock .github/workflows/ci.yml proves byte-identical tested
inputs; no fresh baseline is claimed for ancestry/doc changes. Roadmap current
inventory count corrected to196/869; original25-minute M1 budget labeled initial,
with current35-minute followup evidence retained. Root remains heavy-idle pending
PR creation/reviewer release; required local full6189 applies unchanged bytes.


## Independent PR112 review — 2026-10-01

Isolated detached /private/tmp/suss-review-pr112 atb4df02d with actual pinned
upstreamc4295f30, no symlinks. Read AGENTS, design, roadmap, inventory and handoff;
checked complete source1406/1420/1432, explicit source hashes/metadata/patches/EPL,
dependency initialization and raw f64/Boolean/Number17/Error Wrong arity decoding.
No production defect found. Root6189 and all root handles terminal before slot
release; reviewer exclusively owns cargo/JVM/Node until terminal full result.

All original49 corpus objects and parent corpora preserved. Six additive probes
cover live count/mixing, signed32 overflow, captured helper/live element hash and
Cons cache metadata. Initial30098 terminal1 was an incorrect provisional ordered
overflow assertion(-32); pin false and independent arithmetic identified929.
Corrected new probe source, fresh43535 terminal0:55 exact values/native4,
/private/tmp/suss-pr112-review-oracle55-final.log. Added native test afterward,
focused10323 terminal0/5 /private/tmp/suss-pr112-review-native5.log verifies raw
Number17, second-element throw prevents next/third effects, recovery and Cons
cache/metadata acrossGC. Python82/import105/setup5/reviews196+869/diff pass.
Required full27387 LIVE /private/tmp/suss-pr112-review-full.log, shared target/
build2 cargo test --workspace --locked -- --test-threads=2, noRUSTFLAGS.
No merges/readiness/issue or milestone closure. Exact reviewed-head CI required;
persistent vectors/maps/sets/map entries and remaining M2–M9 gates stay open.


Reviewer full27387 authoritative terminal0; inspected final suss_reader doc tests
and no failed-test/error entries in /private/tmp/suss-pr112-review-full.log.
Required shared-target/build2 --workspace --locked -- --test-threads=2 passes
collection5/55 exact values, unchanged parent corpora/explicit boundaries and ABI45.
Existing manual ignores remain explicit. All reviewer heavy handles30098/43535/
10323/27387 terminal. Reviewer commits/pushes additive probes and evidence, then
releases exclusive heavy slot. Exact final reviewed-head CI gates readiness;
no merge, issue or milestone closure.

### PR112 inherited timeout reconciliation — 2026-10-01

Static rebase of independently revieweddf5e42ba onto independently reviewed
PR111835fe7c4 preserves both parent timeout audits and all collection review
and full-baseline evidence. Scoped git diff verifies crates/runtime/scripts/tests/
oracle/Cargo bytes identicaldf5e42ba. Workflow matches reviewed parent exactly,
inheriting35→60 minutes and rationale; no suite, concurrency or lock gate changes.
Prior full27387 remains identical-code evidence; no fresh baseline is claimed.
Historical exact-head CI36875609291 atdf5e42ba was still in_progress when checked;
it is not a success or new-head certification. New exact reviewed-head CI remains
mandatory. Reviewer113 owns heavy slot full69149; no cargo/JVM/Node started here.
Exact old-head lease protects push. No readiness/merge/issue/milestone closure.

## Vector trie prerequisites — 2026-10-01

Root isolated /private/tmp/suss-core-vector-trie, branch portable/core-vector-trie,
base independently reviewed112df5e42ba, actual detached upstreamc4295f30.
Initial11514 terminal101: missing suss.core/pv-fresh-node located compile failure,
/private/tmp/suss-vector-trie-initial.log. Retained complete VectorNode plusnine
private helper algorithms; explicit fixed defn bootstrap patches/source hashes/EPL
preserve original branches. No native vector replacement or ABI change.
Focused33628 terminal0 clone ownership; fresh73307 terminal0:23 exact pinned
observations/native2, /private/tmp/suss-vector-trie-oracle.log. Added cross-fragment
association/GC and live clone exception19 recovery tests, focused68047 terminal0/4,
/private/tmp/suss-vector-trie-native4.log. Python82/import115/reviews206+859 pass.
All root handles terminal; root exclusively owns local heavy slot for required
workspace baseline next. Full PersistentVector and remaining M2–M9 gates remain
open; no issue/milestone closure. Independent review and final-head CI required.

Expanded95 CI36871799306 cancelled at25m; independent reviewer proved source-tree
identity to reviewed107 and pushed timeout-only35-minute fix447bed49 plus evidence.
Exact CI36877052183 running;95 nowdraft. No test removal or concurrency change.
108/110/111/112 retain their reviewed heads and require exact-head CI before ready.
No root merges performed. User109 cannot resolve in this repository; authoritative
merged96–107 are in the stack branch,93–95 remain open, main remains at91.


Root vector required full22313 authoritative terminal0, inspected final reader
doc tests and no FAILED/error entries in /private/tmp/suss-vector-trie-full.log.
Shared target/build2 cargo test --workspace --locked -- --test-threads=2 passes
vector4/23 exact oracle values, unchanged parent corpora and ABI45. Existing
manual ignores and diagnostic9passes/7knownfailures remain explicit. All root
heavy handles11514/33628/73307/68047/22313 terminal; heavy slot can transfer to
independent reviewer after PR creation. Selection111/artifacts115/reviews206+859.
Next retain actual vector indexing/error and iterator/reduction prerequisites,
then complete PersistentVector. No issue/milestone acceptance or merge claimed.

PR108 CI35-minute run finished enabled workspace tests in34m38 then timed out
compressing post-job cache. Independent reviewer pushed timeout-only60-minute
fix7ffad606 and is rebasing onto95reviewed447bed49; all production/tests unchanged.
Propagate via static reviewer rebases before exact final-head readiness checks.


## Independent PR113 review — 2026-10-01

Isolated detached /private/tmp/suss-review-pr113 at4a19f9a, actual pinned
upstreamc4295f30 without symlinks. Read AGENTS/design/ROADMAP/inventory/handoff;
reviewed complete retained5575–5619/5648–5670, explicit hash-bound defn- patches,
source artifacts/EPL, clone ownership, structural sharing and live lookup order.
No significant production defect found. All root heavy handles terminal before
exclusive cargo/JVM/Node slot transfer.

Original23 corpus objects and all parent corpora unchanged. Six additive probes
cover live new-path fresh/set order, recursive pop child-before-parent cloning,
association parent-before-child cloning, nonempty recursive child preservation,
collapse avoiding cloning and inserted path nil edit. Initial9662 terminal101:
pinned29 matched, native new trace fixture used unsupported js-obj brackets.
Corrected only new trace fixture to numeric GC array; fresh52790 terminal0:
29 exact pinned/native values across4 tests,
/private/tmp/suss-pr113-review-oracle29-final.log. Added native outer-pop-clone
throw23 after successful child cloning; focused60491 terminal0/5 proves raw
one-field Number23, original path intact, restored helper and recovery acrossGC,
/private/tmp/suss-pr113-review-native5.log. Python82/import115/inventory1065/
reviews206+859/diff checks pass. Required shared-target/build2 workspace
baseline69149 LIVE /private/tmp/suss-pr113-review-full.log, noRUSTFLAGS.
No merge, readiness, issue or milestone closure; full PersistentVector and M2–M9
remain unfinished. Exact reviewed-head CI required.


Reviewer113 full69149 authoritative terminal0 through final reader doctests;
no FAILED/error entries in /private/tmp/suss-pr113-review-full.log. Required
shared-target/build2 --workspace --locked -- --test-threads=2 baseline passes
vector5/29 exact values and unchanged parent corpora/explicit boundaries.
Existing manual ignores remain explicit. All heavy handles9662/52790/60491/69149
terminal. Algorithm-byte comparison independently confirms all nine patches only
replace the declaration wrapper. No significant production defect found.
Static parent rebase and exact final reviewed-head CI remain readiness gates.

Reviewer113 static rebase onto reviewed1124a01ab6f preserves both append-only
handoff audits. Scoped git diff --exit-code88981af -- crates runtime scripts tests
Cargo.toml Cargo.lock confirms tested production/native/oracle inputs unchanged.
Workflow exactly matches reviewed parent4a01ab6f, inheriting60-minute budget;
no command, suite, concurrency or lock changes. Prior full69149 evidence applies
unchanged inputs; no new heavy baseline claimed for static ancestry/docs change.
All reviewer heavy handles terminal, exclusive heavy slot released to root.
Exact reviewed-head CI remains pending; no readiness/merge/milestone closure.


## M2 collection literal order decision and lowering — 2026-10-01

Active scope remains finishM2; validated M4 traversal work stays local on
portable/core-vector-traversal at64bef0c, with no PR opened for it. Existing
PR95/108/110/111/112/113 exact reviewed-head CI succeeded and drafts were made
ready; none merged by the agent. New work is on portable/m2-collection-literals
from independently reviewed113head5b2164e1, with real pinned upstream checkout.

User explicitly accepted textual order for unordered literals. The dated design
decision now specifies map key/value interleaving at every size and textual set
entry order, preserving exact pinned differences rather than calling them passes.
Original17 probe sources retained;2 additive raw-order probes bring19 observations
to15 shared values and4 exact deliberate variances,0 skips. Constructor-interface
fixtures are original development-only types, not persistent collection acceptance.
No upstream collection algorithm or inventory item is claimed implemented.

Initial native vector literal failed at HIR lowering after correcting probe-only
EMPTY_NODE/core setup errors. Initial fresh reference duplicate-key setup failure
and subsequent exact set-order mismatch are preserved in
/private/tmp/suss-m2-collection-literal-oracle.log and
/private/tmp/suss-m2-collection-literal-oracle17.log. After the user decision,
35897 authoritative terminal101 proves old keys-first native lowering failed
textual-order expectations, /private/tmp/suss-m2-literals-textual-order-before.log.
Ordered HIR bindings now normalize each large-map key then value before splitting
storage arrays; factories capture owner/method before entries.

22314 terminal0: shared-target/build2 sh scripts/test-collection-literal-oracle.sh
fresh pinned19 exact reference observations and all19 independently decoded Suss
values across4 native tests, /private/tmp/suss-m2-literals-order-oracle19.log.
52552 terminal0: focused portable_collection_literals native5 adds typed language
throws stopping later map/set entries and factory invocation, with GC recovery,
/private/tmp/suss-m2-literals-native5.log. Python86/inventory1065/import115/
reviews206+859/diff checks pass. Reader and namespace audits39472 terminal0:
/private/tmp/suss-m2-reader-audit.log and /private/tmp/suss-m2-namespace-audit.log.

Required full shared-target/build2 workspace --locked -- --test-threads=2 baseline
28575 LIVE, /private/tmp/suss-m2-literals-full.log. No RUSTFLAGS. Root owns exclusive
heavy slot until authoritative terminal result. Independent review and exact-head
CI remain required; no readiness, merge, issue or milestone completion claim.
Refs#9 only partial progress. Remaining literal boundaries: keyword/symbol/quote,
runtime metadata, reader duplicate forms and real persistent collection classes.
Next unblocked M2 task: audit each #8/#9/#11 acceptance criterion against actual
source and executed artifacts, keeping M3 macro/production and M4 collections
requirements distinct; close no issue until all of its criteria have evidence.


Root full28575 authoritative terminal0 through final suss_reader doctests:
/private/tmp/suss-m2-literals-full.log. All enabled workspace tests pass, including
native5/19 exact literal observations; existing manual ignores and the legacy
diagnostic known-failure record remain explicit. Python86/import115/inventory/
review checks passed. No production/native/oracle inputs changed during this full
run. All root heavy handles terminal; release exclusive heavy slot to mandatory
PR114 reviewer after this documentation-only result update is pushed. Final
independently reviewed head and its CI remain readiness gates; no merge or closure.


## Independent PR114 review — 2026-10-01

Isolated /private/tmp/suss-review-pr114 at root docs-only head5a58ba1, with actual
detached upstreamc4295f30. Read AGENTS/design/ROADMAP/inventory/handoff. Reviewed
literal HIR, sequential operand lowering, canonical constructor thresholds,
captured factory owner/methods, metadata/spans and compile-atomic diagnostics.
No significant production defect found. Root full28575 was terminal before the
exclusive cargo/JVM/Node slot transferred to this reviewer.

Added one native regression proving the small-vector constructor and EMPTY_NODE
are captured before an entry redefines the canonical class and mutates its prior
root. The next literal observes the replacement; both nominal identities, roots
and tails remain valid across forced GC. Original19 corpus sources, their exact
expected/reference values and all production inputs remain unchanged.

Fresh25458 authoritative terminal0: shared-target/build2
sh scripts/test-collection-literal-oracle.sh, /private/tmp/suss-pr114-review-oracle19.log,
19 exact pinned observations,15 shared values/4 exact accepted variance
observations/0 skips, and all6 native tests. Python86 checks85438 terminal0;
inventory1065/import115/review overlay206+859/diff checks pass.
Required shared-target/build2 cargo test --workspace --locked -- --test-threads=2
full8004 authoritative terminal0 through final suss_reader doctests,
/private/tmp/suss-pr114-review-full.log. All enabled tests pass; existing manual
ignores and diagnostic known-failure records remain explicit. No RUSTFLAGS.

All reviewer heavy work finished; release exclusive slot to root. Exact final
independently reviewed-head CI remains a readiness gate. No merge, issue closure
or M2/M4 completion claimed. Literal keyword/symbol/quote, runtime metadata,
duplicate-reader forms and real persistent collection types remain documented
boundaries. Next M2 task remains the criterion-by-criterion #8/#9/#11 audit.


## M2 acceptance candidate — 2026-10-01

Previous goal turn made progress: implemented textual vector/map/set literal
lowering, recorded the user's explicit ordering decision, executed fresh19
reference/native observations and full28575 terminal0, opened PR114 and obtained
independent review/full8004 terminal0. Reviewed final114head isddf00b21372982fca7b79c216d988de67da033b4, native6 plus19 exact values and Python86;
exact-head remote CI still gates readiness. No root merge or issue closure.

New isolated portable/m2-acceptance-audit worktree
/private/tmp/suss-m2-acceptance-audit starts at reviewed114ddf00b2 with real
pinned checkout. docs/roadmap/acceptance-m2.md is explicitly a candidate, mapping
all #8/#9/#10/#11 criteria to current source/actual executing tests. No M2 status
or issue closing link is changed before independent criterion review. The
runnable scripts/test-m2-foundation.sh replaces the ineffective proposed
evaluation_order filter with explicit suites, including old emitter guards and
current ABI/namespace/nominal/session/collection interfaces. Full collection
classes, keyword/symbol/quoted expressions, runtime metadata, source
:require-macros and production frontend migration stay explicit boundaries; the
review must decide whether any boundary leaves an actual M2 criterion unfulfilled.
Root has exclusive heavy slot after reviewer114 release; focused acceptance
command will be recorded with its authoritative terminal result. Next task:
dispatch mandatory independent audit of the new PR, implement significant
criterion gaps if found, then require final reviewed-head CI before readiness.

Root focused acceptance18713 LIVE /private/tmp/suss-m2-acceptance-focus.log,
shared-target/build2 sh scripts/test-m2-foundation.sh. Existing reviewed-parent
full8004 is authoritative terminal0 atddf00b2; this audit adds only documentation
and the focused runner, with no Rust/runtime/oracle input changes. Mandatory
new independent reviewer must validate the criterion claims and runner scope;
no Closes links or completion status yet. Root holds heavy slot until18713 terminal.


## PR #115 independent findings and repairs — 2026-10-01

Independent reviewer in /private/tmp/suss-review-pr115 found three significant
legacy backend violations while auditing #9; the initial focused acceptance18713
terminal0 did not cover them. Before-fix79935 terminal101 records actual receiver
result19 rather than23 and bit-and trace21 rather than12; the hash effect probe
failed Wasm validation (expectedi64, foundreference), not an observed repeat count.
Expanded18014 terminal101 records all six bitwise traces21 versus12. No failures
were skipped or relabeled. The real upstream checkout is detached atc4295f3031.

Receiver-first dispatch now saves the receiver before ordered arguments; bitwise
emission keeps the left operand on the Wasm stack while evaluating right; hash
emission captures the input once before every type/sentinel inspection. Existing
hash algorithms are unchanged. Expanded78511 terminal0 executes three regression
functions covering receiver result23, nested/three-operand trace123, all six
bitwise traces12 and asymmetric3/1 results, plus eight hash operand cases.
Large vector emitter loops use disjoint first32/rest slices and do not re-emit an
entry. No portable runtime/oracle inputs or upstream core source changed.

Independent published-criterion review records candidate foundation verdicts in
docs/roadmap/acceptance-m2.md. Source :require-macros/isolated compiled macros are
M3; constructor-interface fixtures prove normalization but not M4 collection
algorithms; reader metadata is distinct from unfinished runtime metadata. Legacy
production frontend/release compatibility is not certified by M2 foundation.
Required full workspace40695 is LIVE at /private/tmp/suss-pr115-review-full.log,
sharedtarget/build2/testthreads2; no other heavy producer. Next: obtain its
terminal result, push significant fixes, then require exact final-head CI and
user merge before closing issues or reconciling default-branch status.


PR #115 independent required full40695 completed authoritatively with exit0
through final reader doctests on source/test heada4bee19. Log:
/private/tmp/suss-pr115-review-full.log. Command:
CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2
cargo test --workspace --locked -- --test-threads=2. Three focused repaired
regression functions78511 also completed terminal0; exact before-fix failures
remain recorded above. No RUSTFLAGS, skips, oracle inputs or algorithm changes.

Independent requirement-by-requirement review proves all published #8/#9/#10/#11
foundation criteria on this reviewed implementation stack. The acceptance audit
explicitly records each verdict and scope; unsupported compiled macro bootstrap,
real collection algorithms, runtime metadata and production migration remain
later gates. This evidence-only commit changes no executed Rust/runtime/test
inputs and carries closing links for #8/#9/#11 so the accepted commit can close
those issues when incorporated into main. #10 remains already closed. Current
ROADMAP/issue statuses remain in progress while stack/CI/user merge are pending.
No GitHub issue/milestone was closed manually and no PR merged. Heavy slot is
released after terminal40695. Next unblocked task: require exact independently
reviewed final-head CI for PR114/115, obtain user merge of the open stack, then
reconcile default-branch M2 status; M3–M9 and deferred hashing#98 remain open.

## M2 integration PR against main — 2026-10-01

The independently reviewed stack ends at
ff4e785c156539e506b69512f1e66c873e72dea3. Its required full workspace baseline
passed (reviewer process40695,843 passed,0 failed,17 existing ignores).
The root also executed `sh scripts/test-m2-foundation.sh` on that final head,
using the shared target and two build workers: process95625 completed with exit0,
529 passed,0 failed,12 existing ignores across18 result groups. Exact criterion
and new emitter repair guards were checked directly in
/private/tmp/suss-m2-acceptance-final-focus.log. Root18713 earlier focused runner
also completed with exit0 before the three missing legacy regressions were added;
that earlier green result alone was insufficient acceptance evidence.

A separate portable/m2-foundation branch now carries the complete reviewed stack
for one integration PR targeting main. Its implementation, runtime, oracle,
provenance and test inputs are byte-identical to reviewed PR115. The new change
only records this integration handoff. Current main remains an ancestor; no
merge or issue closure has been performed. The PR will carry Closes #8/#9/#11
and retain the accepted closing commit, allowing auto-close when this complete
stack reaches main. All published M2 criteria have the independently reviewed
acceptance matrix in docs/roadmap/acceptance-m2.md; future M3–M9 work remains open.

Next gate: dispatch independent integration review, fix any significant findings,
and require exact final reviewed-head CI before readiness. Component PR114/115
also retain their own final-head CI gates. Only the user may merge. Default-branch
M2 statuses remain in progress until the accepted stack is incorporated.

## M3 native compiled command REPL slice — 2026-10-01

Active goal is now full M3, preserving all #12/#13/#14/#15 criteria and design
sections3/6/7/9. Isolated branch portable/m3-persistent-repl in
/private/tmp/suss-m3-persistent-repl starts from current main7a9010b, with real
upstream checkoutc4295f303100bbf5afac449242d30bca1126f1a1. Original repository's
unrelated reference files remain untouched. #115/#116 were merged by the user;
root merged nothing. Rebased #116 head07ff0e6 received independent approval and
CI36912150217 success,843/0/17existing ignores; merged main has identical entire
tree. GitHub issues8/9/11 and M2 milestone are actually closed. ROADMAP/issues.json
and acceptance-m2 now reconcile that foundation evidence; future gates stay open.

Root implemented a bounded native command frontend against portable Session,
with one Store/runtime and independent compiled fragments. No accumulated source,
replayed definitions, fresh runtime per input or recompiled pr-str wrapper.
Printing reads the rooted value; finite numbers use the same runtime formatter,
UTF-16 retains lone surrogates as escapes. Function/type labels are display only,
not compatibility decoding. Unsupported objects produce explicit display errors.
Reader recovery hints accumulate multiline collections/strings/prefixes without a
second lexer. Malformed input reports immediately; incomplete EOF never partially
evaluates. Comment-only input is ignored, :reset replaces session state, and
readline Ctrl-C discards unfinished source. Executing code/pending I/O cancellation
is not implemented. Unreachable replay namespace/printing handlers were removed;
prototype modules are now cfg(test) fixtures, not shipped native frontend code.

Before10094 authoritative terminal101 /private/tmp/suss-m3-repl-before.log:
legacy command rejects global set! in the persistence probe. First after65737
terminal101 found an incorrect test expectation for skipped defonce output;
pinned cljs.core/defonce expands through when-not, so nil is correct. Only that
output expectation changed; bindings/effects stayed independently asserted.
Expanded26340 terminal0 four tests. Multiline6115 terminal101 emittednil/0/0/42
instead of complete-input0/42/1/42/42; reader hints repaired it,16894 terminal0.
An array display probe52013 terminal101 was a too-specific diagnostic assertion:
(array) returns an unsupported native object, not a directly observed empty
reference-array misdecode. Corrected that claim and guarded actual I16 array
storage before string decoding; no fabricated success is recorded.

Final focused70182 terminal0: shared-target/build2
cargo test -p suss-cli --locked --test persistent_repl -- --test-threads=2,
/private/tmp/suss-m3-repl-final-focus.log,7 passed/0 failed/0 ignored. Tests execute
the actual suss binary with stdin/stdout and cover persistence, captured old/new
functions, defonce, failed compile/initializer with preceding effects, exact
binary64/UTF-16 display, multiline source, malformed/EOF recovery, unsupported
object errors and reset/new-input usability. No new upstream implementation was
copied; no shipped JVM/Node dependency or hash algorithm changed.

Required full84639 LIVE: shared-target/build2
cargo test --workspace --locked -- --test-threads=2,
/private/tmp/suss-m3-repl-full.log. Root exclusively owns local cargo/JVM/Node
slot until terminal. No RUSTFLAGS. Native code/test inputs stay frozen during full
baseline; a trailing blank-line diff warning will be fixed after terminal.
Independent PR review, significant fixes and exact final-head CI remain required.
Refs#12/#15 only; do not close from this partial slice.

Next unblocked tasks: supply real GC-owned atom persistence through this frontend
for #12 (full watch/validator/CAS compatibility remains separately tracked M7),
connect namespace commands/reload/cache/private policy for #13, implement isolated
compiled bootstrap/macros and remove the temporary evaluator for #14, then verify
pending-I/O cancellation/reset/retained code vs live heap acceptance for #15.
Tab completion, complete value printing, non-native component and other command/AOT
frontends are explicit boundaries. No M3 issue or milestone completion claimed.

Required full84639 authoritative terminal0 through final reader doctests:
850 passed/0 failed/17 preexisting ignores across72 result groups,
/private/tmp/suss-m3-repl-full.log. All seven new command regressions executed
successfully within that full baseline. Source/test behavior remained frozen;
a trailing extra blank line at main.rs EOF was removed after terminal, with no
semantic code change. Python86 passed, inventory1065 and overlay206reviewed/
859unassessed verified. First combined script11014 ended2 because the final
import-script filename was mistaken, not because those earlier checks failed;
correct python3 scripts/core_import.py --check verified115 files with exit0.
Diff checks now pass. No declaration review was relabeled implemented by this
frontend-only change. All root cargo/JVM/Node work is terminal; heavy slot is
released for the mandatory new PR reviewer. Exact reviewed-head CI remains a
gate. Next implementation task remains real atom persistence for #12; full M3
scope is preserved and no #12/#13/#14/#15 closure is claimed.

## M3 atom persistence regression started — 2026-10-01

PR118 published from be9dd78d8a37c14c3c5a4793ab7c0928c4d07b20, Refs#12/#15.
Independent review_pr118 approves this partial frontend slice with no significant
findings. Reviewer focused85139 terminal0,7passed/0failed/0ignored,
/private/tmp/suss-pr118-review-focus.log; extra command probes verify complete
compile-error isolation, executing fuel-trap recovery, multiline strings and
callable display. Reviewer corrected only an extra probe's constructor-result
expectation (function rather than type); no source or tests changed. CI36917068741
is live on the approved head. Reviewer now watches existing remote CI via17410,
/private/tmp/suss-pr118-ci-watch.log, with no local heavy tools. It must inspect
final logs/head/state before readiness; no merges or M3 closure.

New isolated portable/m3-atom-storage worktree /private/tmp/suss-m3-atom-storage
starts from approved118be9, with real detached upstreamc4295f30. One additional
actual-command regression is intentionally failing, not yet committed/published:
persistent_session_command_repl_keeps_atoms_and_their_captured_values. It covers
atom mutation across inputs, captured old function contents, deref through a
closure and defonce not replacing the existing atom. Before70458 terminal101,
/private/tmp/suss-m3-atoms-before.log, proves located unresolved atom/deref/swap!/
reset! diagnostics; no atom value was executed or claimed successful. Parent7
frontend tests remain unchanged. No production atom code has been added yet.

Root investigated pinned core.cljs Atom4613, atom4642, reset!4661, swap!4694,
IAtom895/IReset898/ISwap903/IWatchable820 and existing retained IDeref/deref. Current
core artifact has no Atom constructor and command Session has no automatic full
core loading. Next implement real GC-owned storage/operations and canonical core
provisioning without source replay, with source/hash/license provenance for any
retained forms and independently decoded GC/order/arity/error evidence. Do not
substitute fake constructor fixtures, no-op watch/validator stubs or unknown
successes for state. Full watcher/validator/CAS compatibility remains separately
tracked M7, but M3 atom persistence must use actual owned values. Preserve the
accepted design; any intended semantic deviation needs a dated decision/evidence.
All root local cargo/JVM/Node handles are terminal; root owns the available heavy
slot. M3 remains active with full #12/#13/#14/#15 acceptance scope.

## M3 compiled atom storage — 2026-10-01

PR118 final approved be9dd78 CI36917068741 completed SUCCESS. Independent
reviewer retrieved final log /private/tmp/suss-pr118-final-ci.log:850passed/0failed/
17existing ignored, all7 command guards and final reader doctests. Readiness
controller4577 and watcher17410 both terminal0. GitHub confirms OPEN/MERGEABLE/
not draft/head unchanged. No merge; issues12–15 and M3 remain open.

Atom child portable/m3-atom-storage is based on approved118be9. Five provenance-
tracked forms now provide real descriptor-backed GC state; runtime ABI2 unchanged.
Session::new_repl compiles the canonical retained core artifact once in its Store
and restores the user namespace. Native REPL uses that profile; reset provisions
a replacement before discarding prior state. Minimal Session::new remains available.
This is a partial core profile, never a claim that pending core dependencies work.

Actual command regression failed before implementation70458 terminal101 with
located unresolved atom/deref/swap!/reset! diagnostics. After95286 terminal0:all8
command tests pass. Native98376 terminal0:3 atom/GC/order/reset guards pass.
Expanded80655 terminal0:8 command/4 native pass including28 independently decoded
source observations. Fresh pinned oracle43883 terminal0 matches all28 exactly,
/private/tmp/suss-m3-atoms-oracle.log. Pinned ordinary redefinition warning retained.
No source constructor fixture, printer/equality success decoder or skip.

Strengthened error decoder39354 terminal101 initially assumed all throws have
ABI2 Error layout, but ex-info uses real nominal ExceptionInfo storage. First
replacement24032 terminal101 did not change the formatted decoder. Corrected
72248 terminal0:8 command/4 native pass, /private/tmp/suss-m3-atoms-final-focus2.log.
Tests separately decode runtime arity Error and ExceptionInfo message/data/cause
fields, assert exact UTF16 messages, unchanged atom state and next-input recovery.

Python/import82602 terminal0:86 tests,1065 inventory declarations verified,211
partial reviews/854 unassessed,120 licensed artifact files verified (116 selections).
Full workspace20454 is running in /private/tmp/suss-m3-atoms-full.log; do not claim
completion until terminal full evidence. Source/tests/runtime inputs frozen during
this baseline. Root owns the only local heavy slot.

Unsupported atom options, IWatchable, validators/watches, protocol fallback,
variadic swap!, reset-vals!/swap-vals!/CAS remain explicit in docs/runtime/atoms.md.
Non-nil validator/watch fields fail before reset mutation instead of silently being
ignored. Complete atom semantics remain M7; M3 progress must still fulfill every
criterion in issues12–15/design sections6–7/9. Next publish this atom branch with
Refs12, mandatory independent review/fixes and exact final-head CI; then namespace
command loading/reload/cache/privacy, isolated compiled macros/versioned bootstrap
and removal of the tree evaluator, running/pending-I/O cancellation and live heap
accounting. Do not merge PRs or claim full M3 from this slice.

Atom full baseline20454 terminal0:855passed/0failed/17preexisting ignored across
73 reported groups through final reader doctests, /private/tmp/suss-m3-atoms-full.log.
Focused source/tests were frozen throughout; only documentation/evidence changed
after focused gates. No RUSTFLAGS. Full command:
CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2 cargo
test --workspace --locked -- --test-threads=2. Local heavy slot is released pending
mandatory atom PR review. No issue closure/readiness from local success alone.

## M3 namespace command/reload implementation — 2026-10-01

PR119 published draft at0d8f086d63dfb96601b9a93e51d5b75d2e5c7476, base118be9,
https://github.com/bobby/suss/pull/119. Independent review_pr119 approves exact
head with no significant findings/edits. Reviewer88909 terminal0:12focused;
fresh oracle20271 terminal0:28exact observations/4native, import120 verified; extra
real command probes cover nested mutation/self-cycle/UTF16/old contents/hash/reset.
Reviewer corrected only its extra probe reset-nil/surrogate-case expectations.
CI36921984358 live; watcher35617/controller75563 verify final exacthead/all jobs/
855passed/0failed/17ignored/finalreader/OPEN/MERGEABLE before readiness. No merge.
Issue12 comment5939967631 and15comment5939968009 record partial progress.

Root isolated child portable/m3-namespace-reload worktree at1190d8f086 with actual
pinned submodule. Before40796 terminal101, /private/tmp/suss-m3-namespace-before.log:
new command regression fails with located unsupported load/reload/unresolved
namespace diagnostics. After38250 terminal0 actualcommand1 passes. Added native
reload proof3766 terminal0:namespace3/command8/session33 pass. Strengthened18208
terminal0:namespace4/command8/session33 pass, /private/tmp/suss-m3-namespace-final-focus.log.
No ignored tests or discarded failures. Removed unrelated rustfmt changes before
the final focused gate; only native main command dispatch/session API/tests change.

Session::reload_namespace reuses cells and fresh source snapshots, reloads either
only target or its reachable dependency graph, preserves caller scope and unrelated
loaded modules, invalidates selected loaded identities only after successful
preparation/linking, and marks each initialized module loaded independently.
Compile failure preserves prior state; initializer failure preserves old binding
and earlier arbitrary effects; retry keeps successful dependencies. New command
load/reload/reload-all/in-ns returns nil without displaying initializer values.
The supplied core profile reload is explicitly rejected in favor of reset.
Source libspec reload metadata, macro phase imports, full private Var policy,
complete source/artifact/macro caches remain unfinished. No ABI/dependency change
or copied implementation. docs/runtime/namespace-session.md records exact policy.

Next run full frozen workspace baseline and Python/provenance checks, publish
Refs13/Refs12 stacked draft, independent review/fixes/exact CI. Then implement
compiled macro session/bootstrap/&form/&env/syntaxquote/gensyms/phase graph/cache
and remove old evaluator, plus running/pending-I/O cancellation/live-memory
accounting. Full M3 goal remains active; no issue closure or milestone completion.

Namespace checks58841:Python86/inventory1065/overlay211+854/import120/offline
roadmap preview all pass. Same handle58841 now runs full workspace with frozen
source/tests; /private/tmp/suss-m3-namespace-full.log. Re-poll this handle until
terminal; do not restart solely for an observation timeout. Atom review has
released local heavy slot; root owns it for this baseline. PR119 exact CI watcher
35617/controller75563 remain live at roughly9minutes with full workspace running.
Current namespace child is uncommitted/unpublished pending full verification;
next publish and dispatch independent review before readiness. No M3 closure.

Namespace full58841 terminal0:859passed/0failed/17existing ignored across74
reported groups through finalreader docs, /private/tmp/suss-m3-namespace-full.log.
Source/tests were frozen throughout the full command:
CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2 cargo
test --workspace --locked -- --test-threads=2. No RUSTFLAGS; no new ignored guards.
Root heavy slot is released pending independent namespace PR review.

## M3 quoted identifier/list data prerequisite — 2026-10-01

PR1190d8f086 final CI36921984358 SUCCESS:855/0/17/73groups/finalreader/newguards
verified independently. Initial observer75563 terminal1 was ANSI around reader
doctests; normalization observer45174 terminal0 completed readiness using same
run/no restart/no source edits. GitHub OPEN/MERGEABLE/notdraft/headunchanged.
PR12010ad0de final CI36923691274 SUCCESS:859/0/17/74groups/finalreader/4namespace
guards verified. Reviewer16182 terminal0:45focused; first wrong testtarget failed
before execution and was corrected. Initialobserver36172 ANSI failure corrected
88243 terminal0 on same run. GitHub OPEN/MERGEABLE/notdraft/headunchanged; no
significant findings/source fixes. No merges/issueclosures. Progress comments13
5940195826 and12 5940196364 published. Both reviewers release heavy slot.

New isolated portable/m3-quoted-identifiers child worktree starts at12010ad0de with
real pinned upstreamc4295f30. This is a necessary compiled-macro language-data
prerequisite, not alternative host syntax or macro interpreter. Before71794
terminal101 unresolved quote, /private/tmp/suss-m3-quotes-before.log. First42013
terminal101 core source js/Error unresolved; name/namespace patches now use typed
private runtime error construction, preserve source branches/pending str_ dependency.
90453 terminal0 one nestedSymbol/Keyword/list/UTF16/GC regression passes.

Twelve source selections retain full types/protocols/predicate/name/equality/hash
methods, hashes and EPL; all in-progress. Compiler adds canonical constructor and
quote/list lowering, exact pinned cached identifier hashes using UTF16/signed32
arithmetic (EPL notice/provenance in originalRust quotes.rs). No modern hash change
or prototype xxHash use. Full constructor conversions/get/str_/writer branches
remain pending, not fake implementations or compatibility claims.

Fresh primary87836 matched all48 source cases, then terminal101 in native decoder
class-layout assumption. Backtrace probe terminal101 and93218 terminal101 identify
closure-owned property wrapper, not a production value mismatch. Native68198
terminal0 validates actual wrapper/descriptor and matches all48 independently
decoded values. Stronger87866 terminal0:quotes3/command8/atoms4/namespace4 pass,
/private/tmp/suss-m3-quotes-final-focus.log. Invalid quote arities/metadata/collections
retain located errors and compile atomicity; quoted effects do not execute, and
owned identifier roots survive unbinding/GC. No ignored guards.

Next full frozen baseline/import/Python checks and independent PR review/CI.
Quoted vector/map/set and runtime metadata still require actual persistent types;
then isolate compiled macro session using same pipeline, syntaxquote/gensyms/
&form/&env, phase dependencies/versioned Java-free bootstrap/cache invalidation
and remove old evaluator only after acceptance. #15 pending-I/O/running cancellation/
cleanup/live memory remains unfinished. All #12–#15/M3 scope stays active.

Quote full8386 terminal0:862passed/0failed/17existing ignored/75groups through
finalreader doctests, /private/tmp/suss-m3-quotes-full.log. Frozen source/runtime/
tests; only docs and development oracle variable naming changed during full.
Python86/inventory1065/reviews223+842/import132/offline roadmap preview all pass.
Final combined fresh pinned oracle46204 terminal0:48exact observations and3native
guards, /private/tmp/suss-m3-quotes-final-oracle.log; primary private-var warning
retained. Root local heavy handles all terminal and slot released for mandatory
quote PR review. No milestone/issue closure from this prerequisite.

## M3 compiled phase-session execution in progress — 2026-10-01

Authoritative GitHub main613d6f8 includes merged118/119/120 and is tree-identical
with reviewed12010ad0de. Quote commit was moved without tree changes onto main:
PR122 ed839062, independent review dispatched; CI36928085693 confirmed live.
Issue14 progress comment5940891653 records partial quote data and all remaining
macro acceptance requirements. PR stays draft until review/fixes/final-head CI.

Isolated child portable/m3-phase-session now has two proposed executing regression
tests for separate Runtime/Macro Stores, phase-qualified core/global cells, owned
root rejection, mutations, source dependencies/reload/error recovery and reset.
Before/after commands remain pending while mandatory PR122 reviewer owns the
exclusive local heavy slot. Native Session presently hardcodes PhaseRuntime; next
implement explicit phase construction and preserve it across every host operation.
This is an execution prerequisite, not completed macro expansion: defmacro/source
require-macros, syntaxquote/unquote/splicing/gensyms/&form/&env, collection/metadata
macro data, versioned Java-free bootstrap, complete cache invalidation and legacy
evaluator removal are still required. Cancellation/pending I/O/live GC accounting
also remain open M3 work. No issue closure or milestone completion.

Phase before78140 terminal101: three missing native APIs,
/private/tmp/suss-m3-phase-before.log. After66092 terminal0: first two actual
Runtime/Macro execution tests pass, /private/tmp/suss-m3-phase-after.log.
Session now owns immutable phase; all intrinsic cells/module plans/input/scope/
reload/reset use it. Existing constructors remain Runtime; new_macro provisions
retained source in its isolated Macro Store. No compiler/ABI/dependency change.
Added third compiled transformer invocation and failed-reset preservation test.
Final focused15761 is live:compiled_phase_session/persistent_session/portable_atoms/
namespace_session/persistent_repl, /private/tmp/suss-m3-phase-final-focus.log.
Re-poll same handle; no full until focused success. Root owns local heavy slot.

PR122 independent reviewer approves ed839062 with no significant findings/fixes.
Temporary probe94117 terminal0 covers deep quoted lists through48 nesting, bounded
reader errors63/64/65/100 with unchanged stats, and128-wide GC-rooted list/recovery.
Temporary probe removed; quote worktree clean. Import132/reviews223+842/provenance
verified. CI36928085693 still live; reviewer owns remote readiness gate only and
has explicitly released heavy slot. No merge/closure.

Final phase focused15761 terminal0:3phase/4namespace/8command/33persistent/4atoms
(52pass/0fail/0ignore), /private/tmp/suss-m3-phase-final-focus.log. Source/tests
frozen. Full63172 is live, /private/tmp/suss-m3-phase-full.log, required command:
CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2 cargo
test --workspace --locked -- --test-threads=2. Same handle must be observed until
terminal. Python/provenance36774 terminal0:86tests/inventory1065/reviews223+842/
import132/offline roadmap preview. Real pinned upstream submodule initialized;
local shallow reference initialization failed before clone and direct pinned
checkout58549 succeeded. No symlink, no hidden provenance failure.

Next after full success publish phase prerequisite stacked on122, dispatch its
independent review and require final-head CI. Then integrate actual compiled
source macro expansion/phase edges and remaining language data/bootstrap/cache;
keep original all-requirements M3 scope. No evaluator deletion before acceptance.

Phase full63172 terminal0:865passed/0failed/17existing ignores/76groups through
final reader doctests, /private/tmp/suss-m3-phase-full.log. Source/tests frozen;
only docs/issue evidence changed during full. No RUSTFLAGS/new skips/ABI changes.
Root heavy slot released for mandatory independent phase PR review. Publication
uses Refs14/13/15; source macro/bootstrap/cancellation acceptance remains open.

## M3 compiled macro form/data integration — 2026-10-01

PR122 ed839062 independent review approved; CI36928085693 SUCCESS exacthead,
862passed/0failed/17existing ignores/75groups/finalreader/3new guards verified.
Watcher51625 terminal0 and root marked122ready; API OPEN/MERGEABLE/notdraft/head
unchanged. PR123241c9b9 independent review approved; focused82969 terminal0:40.
Temporary50674 terminal0 verifies sharedEngine/actual macro imports/no runtime
imports/foreign arguments/exceptions/dynamic restoration/reset. Tree clean;
reviewer remote CI watcher75917 live on36929435478; root heavy slot is free.
Issue14 comment5941078615 tracks partial phase execution; no merge/closure.

Root child portable/m3-macro-forms at123241c9b9 has a real pinned submodule91464
terminal0. Before47755 terminal101:missing eval_forms API. After91666 terminal0:
three native scalar/spans/metadata/dependency regressions pass. Source and owned
forms now share selected-form namespace/HIR/IR/module preparation; input no longer
rereads source after dependency discovery. Existing source behavior stays enabled.

Bridge-before command terminal101:missing portable_macro_data module. After81505
terminal0:compiled transformer returns real List data, native reads Forms and
Runtime compiles/executes result42. FormBridge captures canonical source class
roots and validates nominal storage, with source span errors, no native macro body
interpretation. Five roots survive rebinding/GC and are invalid after reset.

Negative first check terminal101: budget signature edits incomplete; corrected.
Negative91690 terminal101:extra probe attempted second ns directive inside one
input, unsupported by existing grammar. Corrected probe uses enter_namespace;
no production behavior changed for that expectation. Bound probe first command
terminal101:ArrayRef allocator argument incorrect. Corrected ArrayRefPre use.
Bounds13805 terminal0:all6native tests pass, including cycles and total UTF16
allocation guard. Finalfocus4642 terminal0:57native tests pass. Strengthened
62147 now live after adding128-wide GC data and65-deep runtime-data bound; log
/private/tmp/suss-m3-forms-final-focus2.log. Re-poll same handle until terminal.

Root owns heavy slot until focus/full complete. Source/tests not frozen until
62147 passes. Next full frozen workspace baseline/Python/provenance, publish
Refs14/12/13/15 draft, independent review/significant fixes/exactfinalheadCI.
Then integrate source macros and required complete data/bootstrap/cache behavior;
all explicit M3 acceptance remains intact. MacroEvaluator still present; no
scripts/verify-bootstrap.sh exists yet. docs/runtime/compiled-macro-forms.md records
metadata/collections/sequence data gaps instead of certifying incomplete support.

Strengthened focus62147 terminal0:57pass/0fail/0ignore across forms6/phase3/namespace4/
command8/persistent33/quote3. Frozen counter regression96253 terminal0:forms6
pass; compiled transformer effects observed exactly1 after result transport and
runtime compilation. Full59225 now live on frozen source/tests,
/private/tmp/suss-m3-forms-full.log, full required workspace command/sharedtarget/
2jobs/no RUSTFLAGS. Checks39817 also live; re-poll both handles rather than restart.
Only docs/issue evidence change during full. Root still owns heavy slot.

Forms full59225 terminal0:871passed/0failed/17existing ignores/77groups through
finalreader doctests, /private/tmp/suss-m3-forms-full.log. Source/tests frozen; only
docs/issue evidence changed during full. Checks39817 terminal0:Python86/inventory
1065/reviews223+842/import132/offline roadmap preview. No new skips/RUSTFLAGS/ABI/
dependency/source selection changes. Root heavy slot released for independent
forms/data PR review. Source macro expansion and all remaining named acceptance
stay required; this transport is not a completed bootstrap or interpreter substitute.

PR123 exact reviewed241c9b9 CI36929435478 SUCCESS:865/0/17/76groups/finalreader/
3newphase guards verified. Watch75917 and log95923 terminal0; independent review
no significant findings/fixes; tree clean. Root marked123ready; API OPEN/MERGEABLE/
notdraft/exacthead. PR122 also ready; neither merged and no issues closed.

## M3 source macro execution/expansion in progress

Root isolated child portable/m3-source-macros starts at1249295c8b. PR124 independent
review approves exacthead with45focused plus independent storage/budget/parity
probes; no findings/fixes. CI36932620871 watcher91480 confirmed live. Heavy slot
released by reviewer; root owns it for source macro checks.

Before30755 terminal101 missing CompiledMacros/eval_with_macros APIs. First72944
terminal101 callback routed through wrong prepare_modules wrapper; corrected.
After59174 terminal0 two native source definition/expansion tests pass. Focus23946
terminal0:48pass across source2/forms6/phase3/namespace4/persistent33. Stronger71692
terminal0:source3 verifies actual dependency artifact expansion/redefinition/failed
replacement. Logs/private/tmp/suss-m3-source-macros-{before,after,after2,focus,frozen-focus}.log.
Real pinned submodule36983 terminal0. Partial defmacro review adds224/841;132 licensed
artifacts only change review manifest hash. docs/runtime/compiled-source-macros.md
records pinned SHA/lines, original Rust and every unsupported definition feature.

ExpansionHost is invoked from real HIR lexical analysis, preserving local/field
shadowing, source order, quote bypass and bounded recursion. Source and dependency
fragments use the same host/pipeline. Native CompiledMacros compiles macro bodies
into its separate Macro Store, marshals actual &form/arguments and reads actual
returned data. No native macro body evaluator. Missing &env remains compile error;
no fake nil map. Explicit registration only; aliases/require-macros, macro-body
expansion, full defmacro signatures/attrs/marker metadata and prompt/AOT integration
remain unfinished. Syntaxquote/gensyms/full form-env metadata/collections/bootstrap/
cache/evaluator removal and #15 cancellation/live accounting are still required.

Source/tests now frozen; full workspace baseline live, log
/private/tmp/suss-m3-source-macros-full.log. Re-poll returned handle, never restart
from an observation timeout. After full/Python/provenance success publish draft
Refs14/13/12 and dispatch independent review/fixes/exact final-head CI. No M3 closure.

Source macro full34035 terminal0:874passed/0failed/17existing ignores/78groups
through finalreader doctests, /private/tmp/suss-m3-source-macros-full.log. Checks69553
terminal0:Python86, inventory1065, reviews224/841, licensed import132 and offline
roadmap preview pass. No added skips, ABI/dependency changes or RUSTFLAGS. Root
heavy slot released. Publish draft and require independent review plus exact-head
CI; full M3 acceptance, automatic macro loading and bootstrap remain unfinished.

## PR125 independent review fix — source macro special forms

Independent review of515457ae found that registered loop* macros intercepted the
compiler's true unqualified loop* special form. The failing regression36892
terminal101 reached macro argument transport and rejected its bindings vector;
ordinary loop*/recur behavior must retain special-form priority. CompiledMacros
now protects loop* alongside the existing special forms. The regression also
executes qualified user/loop* as a normal macro lookup. Focused83427 terminal0:
4 compiled_source_macros tests pass. Original focused61573 terminal0:16pass
(forms6/phase3/source3/namespace4). Temporary independent probes3898 terminal0:
tail recur through macro expansion/non-tail rejection, qualified lookup despite
local shadowing, macro exception compile atomicity/recovery, real lexical callback
context, textual analysis order and quoted-data bypass. Probe source retained at
/private/tmp/suss-pr125-independent-probe.rs; temporary test removed.

Review fix requires a replacement-head full workspace baseline and exact final-head
CI before readiness. No merge or M3 completion; all full macro/bootstrap/lifecycle
acceptance remains open.


## M3 definition signatures and macro-body expansion in progress

PR124 exact9295c8b independently approved and CI36932620871 SUCCESS871/0/17/77,
all six guards/finalreader; watcher91480/log73134 terminal0. Root marked ready,
API OPEN/notdraft/headsame. No merge or issue closure.

PR125 opened at515457a with full874/0/17/78. Independent review found macro named
loop* could intercept a true special form. Reviewer added regression before36892
terminal101/after83427 terminal0 four source tests; temporary3898 two meaningful
lexical/tail/order/exception probes pass. Fix81bfbadd pushed. Required replacement
full49081 is live /private/tmp/suss-pr125-reviewed-full.log, exact-head CI36935692730
watcher27273 live. Reviewer owns exclusive heavy slot; no competing Cargo commands.
Do not mark125ready until terminal full plus exact-head CI verification.

Root child /private/tmp/suss-m3-macro-signatures portable/m3-macro-signatures now
bases directly on81bfbadd (temporary identical-tree cherry418e8c9 normalized by
verified soft reset). Uncommitted definition parser supports single/multiple/variadic
signatures, docs and ordinary source attribute maps; attributes stay Forms metadata,
not fake observable runtime maps. Privacy/const/macro/export still explicit errors.
Namespace enter/identity delegates actual Macro Store environment and keeps roots.

Before40116 terminal101 missing namespace APIs, log macro-definitions-before.
Firstafter3222 terminal101 correctly rejected :private true, rather than silently
accepting unsupported privacy policy. Regression now uses ordinary :added attrs and
asserts privacy rejection. Focus85219 terminal0 definitions2/source4, log
/private/tmp/suss-m3-macro-definitions-after2.log. Root returned heavy slot to reviewer.
Since that focus, added owned CompilationSnapshot and real macro-body expansion
through same HIR host/original Macro Store, a third definition regression and
single multi-signature edge fix. THESE LAST EDITS ARE NOT YET RUST-TESTED. Next run
focused definitions/source/forms/phase/persistent, resolve failures then freeze and
run required full baseline. Do not publish or claim current child tests passed.

Actual pinned submodule47317 terminal0. Provenance initial check failed because new
submodule was uninitialized; real pin initialized, not symlinked. Import132/inventory
1065 pass; reviews --check was invalid CLI usage, corrected plain cljs_reviews.py
passes224/841. Python11550 terminal0:86 tests. No source selection/new skips/ABI
changes. &env/metadata collections/automatic macro imports/syntaxquote/gensyms/
versioned Java-free bootstrap/full cache/evaluator removal and #15 full cancellation/
live accounting remain required. Milestone issue14 progress comment5942000424.


Reviewer PR125 full49081 terminal0:875/0/17existing ignores/78groups/finalreader,
all4 source guards. Exact81bfbadd approved after significant fix; CI36935692730
watch27273 remains live. Heavy slot released to root.

Root final child focus44580 terminal0:53passed/0failed/0ignored across definitions3/
forms6/phase3/source4/namespace4/persistent33, log
/private/tmp/suss-m3-macro-definitions-focus.log. New macro-body snapshot/expansion
and single multi-signature edge now actually execute. Source/tests frozen; next
required full workspace baseline. Only docs/evidence may change during full.

Frozen definition full96199 now live, /private/tmp/suss-m3-macro-definitions-full.log;
root owns exclusive heavy slot. Re-poll same handle to terminal. After success
finish docs/evidence, publish child draft, dispatch independent review/fixes and
require exact-head CI. All remaining original M3 criteria stay open.

Definition full96199 terminal0:878passed/0failed/17existing ignores/79groups
through finalreader doctests, /private/tmp/suss-m3-macro-definitions-full.log.
Focused53 and Python86/provenance pass. Source/tests stayed frozen; no added
skips/ABI/dependency/source-selection changes. Ready to publish draft and dispatch
independent review/significant fixes/exact reviewed-head CI. All M3 gaps remain.


## M3 IndexedSeq macro data transport in progress

Root /private/tmp/suss-m3-indexed-macro-data portable/m3-indexed-macro-data bases
on126825b32f (fast-forward after initial focus on81bfbadd). PR126 independent
review approves825b32f, no significant findings; first56598 terminal101 saw stale
shared-target library, mtimes forced correct rebuild;20993 terminal0:17 focused/
independent probe cases, clean tree, heavy slot released. CI36937011198 watcher16832
still live. PR125 exact81bfbadd also approved/full875/0/17; CI36935692730/watch27273
still live. Neither ready until final-head CI; no merges. Issue14comment5942254370.

Indexed before97652 terminal101:all3 regressions reject actual IndexedSeq data.
After/after2 terminal101:wrong Wasmtime StorageType::Val variant; correctedValType.
After3 handle45714 terminal101:2pass, GC/root test used forbidden cross-namespace
qualified def; corrected probe to ns/enter_namespace, no production policy change.
Focus21327 terminal0:16pass data3/forms6/phase3/source4. Real pinned submodule17640
terminal0. Root then added exact4094/4095 node boundary and structural impostor
regressions, inherited126, forced cli source mtimes to avoid shared-target stale
library. Finalfocus69472 terminal0:52pass across indexed3/definitions3/forms6/phase3/
source4/persistent33, nofail/ignore, /private/tmp/suss-m3-indexed-data-final-focus.log.

Actual source IndexedSeq class root plus private array sample identity are captured
once (seven bridge roots). Decoding validates nominal/storage/layout/finite integral
index/metadata and reads only suffix with shared node/unit budgets. Strings preserve
one UTF16 unit per sequence string including surrogate halves. Cons can terminate
in IndexedSeq. Actual variadic macro returns arguments as code and executes42.
Raw arrays/impostors/malformed indices/storage/metadata/cycles/oversized outputs
reject at supplied callsite. All persistent vectors/maps/sets/metadata/lazy sequences/
&env and remaining macro bootstrap/cache/phase loading acceptance stay required.

Source/tests now frozen. Run required full workspace baseline before publishing.
Only docs/evidence may change during it. Python/provenance:inventory1065/reviews224841/
import132 verified, selection unchanged. Next complete full/Python then draft PR,
mandatory independent review/significant fixes/exact final-head CI. Original M3
requirements including #15 cancellation/pending I/O/live accounting stay open.

Frozen Indexed full42871 now live, /private/tmp/suss-m3-indexed-data-full.log.
Root owns exclusive heavy slot; re-poll42871 to terminal rather than restarting.

Indexed Python32115 terminal0:86tests pass; finaldiffcheck clean. Full42871
authoritatively live, progressed into portable_collection_hash; no observed failure.
PR126 watcher16832 and PR125 watcher27273 confirmed live on their exact reviewed
heads; remote CI is the remaining readiness gate. No merges or issue closures.

Indexed full42871 terminal0:881passed/0failed/17existing ignores/80groups
through finalreader doctests, /private/tmp/suss-m3-indexed-data-full.log. Frozen
source/tests preserved;52focused/Python86/inventory1065/reviews224841/import132
pass. Publish draft then independent review/fixes/exact reviewed-head CI.


## Native REPL compiled macro integration in progress

PR1271169a04 published after881/0/17/80full; independent review approves exacthead,
no significant findings. Probe/focus89228 terminal0:20pass, including suffix-only
reads skipping10000-cell and unsupported prefixes, signed zero/UTF16/empty suffix,
nested cycles/foreign/reset roots. Tree clean. CI36938307507 watcher23236 live;
PR126825b32f CI36937011198 watcher16832 still live. No readiness without final CI.

PR125 review fix81bfbadd exact CI36935692730 SUCCESS875/0/17/78/alljobs/four guards/
Python86/finalreader. Watch27273/log21147 terminal0; root markedready, API OPEN/
notdraft/headsame. No merge or issue closure.

Root child /private/tmp/suss-m3-repl-macros portable/m3-repl-macros bases on1271169a04.
Before3261 terminal101:two actual native command macro regressions failed; first
implementation after29042 terminal0 macro2/oldcommand8. Added load/reload, reset
second-phase failure and alias/conditional/shadow tests. Load-before92881 terminal101
saw stale shared-target library, touch relevant cli source mtimes forced correct
rebuild; before2 15199 terminal101:4pass/one genuine load macro unresolved failure.

Module prepare_modules_with_expander now supplies same lexical host to load/reload;
existing APIs delegate no expansion. Native command owns Runtime+Macro Stores,
registers standalone parsed selected definitions, formats actual Macro function
result, and compiles later owned forms without printing/reparse/replay. Resolved
runtime defmacro bindings retain call semantics. Reset creates both replacements
before discarding either phase; macro replacement failure preserves old runtime
roots/bindings/macros. Namespace registration tracks current Runtime namespace.
No fake runtime macro Var/marker or metadata/environment maps.

Focus81460 terminal0:24pass command5/oldcommand8/namespace4/definitions3/source4.
Broader53696 terminal0:69pass across9native suites, nofails/ignores, log
/private/tmp/suss-m3-repl-macros-final-focus.log. Compiler module56662 terminal0:
11pass, /private/tmp/suss-m3-repl-macros-module-focus.log. Main edits after broad
focus only formatting; semantic source/tests frozen now. Realpin63823 terminal0;
import132/inventory1065/reviews224841 pass. PendingPython and required frozenfull.

Four actual command regressions plus one native host reset-failure regression
record current subset. docs/runtime/compiled-repl-macros.md lists mixed/nested
macro definitions, automatic phase imports/alias/privacy, complete AOT, &env/full
&form metadata, syntaxquote/gensyms/splicing, persistent/lazy data, versioned
Java-free bootstrap/cache/evaluator removal and #15 cancellation/live accounting
as still required. No scripts/verify-bootstrap.sh exists. Next full/Python pass,
publish draft, dispatch independent review/significant fixes/exact-head CI; then
continue automatic macro phase loading and full language-data/bootstrap acceptance.

Frozen native REPL full88080 now live, /private/tmp/suss-m3-repl-macros-full.log;
root owns exclusive heavy slot. Re-poll same handle to terminal, never restart
solely from timeout. Python4053 terminal0:86pass; finaldiffcheck clean.
PR126 exact reviewed825b32f CI36937011198 SUCCESS878/0/17/79/alljobs/all3guards/
finalreader, watch16832/log45004 terminal0. Root markedready; API OPEN/notdraft/
headsame. PR1271169a04 independently approved/no significant findings, CI36938307507
watch23236 still live. No merges or issue closures.


First REPL frozen full88080 terminal0:886/0/17existing ignores/81groups/finalreader.
Before publication root inspection found core exclusions/aliases not honored by
standalone definition dispatch. Added sixth native command regression. Before14436
terminal101:5pass/one genuine namespace failure, log
/private/tmp/suss-m3-repl-macros-alias-before.log. Environment now supplies explicit
bootstrap-origin resolution respecting aliases/refers/exclusions/own bindings;
canonicalize only the definition head with original spans/metadata. No phantom
runtime bindings/values created. Qualified core bootstrap remains explicit.

Finalfocus70944 terminal0:70pass/0fail/0ignore across9native suites including6new
command/host guards, log /private/tmp/suss-m3-repl-macros-final-focus2.log. First
886baseline is not final-head evidence after this semantic fix. Run module focus
then a new frozen full required baseline; no publication until that succeeds.
Source/tests frozen again. Core/Phase import policy and all original M3 gaps stay
required. Issue14progress comment5942545379 records127 and open criteria.

Module focus84836 terminal0:11pass after alias/exclusion fix. Replacement frozen
full now live, /private/tmp/suss-m3-repl-macros-full2.log; record returned handle
and re-poll to terminal. Root owns exclusive heavy slot. Only docs/evidence may
change while full runs. No RUSTFLAGS/new skips/ABI/dependency/selection changes.

Replacement native REPL full31177 live, /private/tmp/suss-m3-repl-macros-full2.log;
re-poll31177 to terminal. Root owns exclusive heavy slot. Source/tests frozen.
PR127 exact reviewed1169a04 CI36938307507 SUCCESS881/0/17/80/alljobs/all3guards/
Python86/finalreader; watcher23236/log92926 terminal0. Root markedready, API OPEN/
notdraft/headsame. PR125/126 also ready; none merged and all M3 issues stay open.

Replacement native REPL full31177 terminal0:887passed/0failed/17existing ignores/81groups through final reader doctests. Evidence: /private/tmp/suss-m3-repl-macros-full2.log. Final focused70 and compiler modules11 also pass after namespace alias/exclusion fix. Heavy slot released. Publish draft and require independent review, significant fixes and exact reviewed-head CI before ready. M3 remains in progress with all outstanding acceptance criteria preserved.

PR128 https://github.com/bobby/suss/pull/128 draft head3fa84b5 bases on1271169a04.
Full31177 terminal0887/0/17/81, native70/modules11/Python86/provenance pass.
Independent review_pr128 owns heavy slot. CI36941605461 confirmed in_progress
on3fa84b5; readiness pending review/fixes/exact-head CI. Issue14 comment5942747446.
Root child /private/tmp/suss-m3-macro-imports portable/m3-macro-imports starts
actual native command regression compiled_macro_imports: automatic tools macro
namespace requiring ordinary Macro-phase helper, aliases/refers/renames and
separate Runtime helper cells during source reload. Not yet executed; current
strict :require-macros rejection means implementation remains required. No skip
or success claim. Next run focused before-test when review releases heavy slot,
then integrate actual phase graph/source definitions/publication policy across
compiler and isolated Macro host; do not erase clause or fabricate loaded state.

Macro imports before97332 terminal101: real command rejects source clause;
/private/tmp/suss-m3-macro-imports-before.log. Phase discovery58288 terminal0
14pass (three new graph + eleven executing ordinary module tests). Implemented
compiled host source imports, phase-qualified provided identities, Macro source
helpers/definitions executed once in textual order, catalogs/aliases/refers separate
from Runtime cells. Initial after83774 terminal0 automatic command1. Focus17324
terminal0 20pass macro imports3/repl6/definitions3/source4/namespace4. Broader96269
terminal101: fourth new test reached known unsupported List display; revised that
regression to observe all four scalar functions individually rather than assert
unsupported formatting. No skip or lost namespace/shadowing observation.
Final focus39213 live; root owns exclusive heavy slot. Source macro reload/
privacy/inference/AOT/cache/bootstrap and all full M3 criteria remain required.
PR128 independent review exact3fa84b5 approves no significant findings; 17 existing
tests plus3 independent probes pass, heavy slot released. CI36941605461 remains
in_progress; no readiness/merge/issue closure.

Final macro imports focus39213 terminal0:66pass/0fail/0ignore across nine native
suites, /private/tmp/suss-m3-macro-imports-final-focus2.log. Module focus39807
terminal0:14pass (existing11/newphase3), /private/tmp/suss-m3-macro-imports-module-focus2.log.
Source/tests now frozen. Required full18144 live, /private/tmp/suss-m3-macro-imports-full.log;
root owns exclusive heavy slot; re-poll18144 to terminal without restart from
observation timeout. Python95463 live, /private/tmp/suss-m3-macro-imports-python.log.
Inventory1065/reviews224841/import132 verified after partial review update; no
selected source/ABI/dependency/new skip changes. Only docs/evidence may change
while full runs. Before any PR publication require full/Python pass, independent
review/significant fixes and exact reviewed-head CI. Next complete that gate,
then integrate source macro reload/cache/privacy and remaining complete bootstrap,
&form/&env/syntaxquote/gensyms, evaluator removal and #15 scheduler/cancellation/
pending-I/O/live GC accounting acceptance. No milestone closure or merge.

Macro imports Python95463 terminal0:86pass, no errors. Full18144 confirmed live
and progressing through native persistent_session, not terminal. PR128 CI watcher
93300 live, /private/tmp/suss-pr128-final-ci-watch.log, exact approved3fa84b5.
Issue14 partial progress comment5942922285 records imports/focused evidence and
remaining original acceptance. Do not claim full/CI success before terminal logs.

Macro imports full18144 terminal0:894passed/0failed/17existing ignores/83groups through finalreader doctests, /private/tmp/suss-m3-macro-imports-full.log. Source/tests frozen; focus66/module14/Python86/provenance pass. Heavy slot released. Publish draft; require independent review/significant fixes/exact final-head CI before readiness. All remaining original M3 acceptance requirements stay open.


## PR129 independent review fix — Macro dependency alias/refers

Independent review of2a2912b reproduced a significant source macro-body bug:
helper.sus defines `(defmacro forty [] 40)`; tools.sus ordinarily requires that
Macro-phase dependency using an alias and a renamed refer. The compiler resolved
both names as ordinary calls to the implicit-&form function because actual
registered helper roots were absent from the Macro export catalog. New native
command regression failed before the fix41903 terminal101 with both macro calls
throwing. No success/skip was substituted.

Successful compiled definitions now publish only actual registered roots into
the Macro Store environment's export catalog. Macro-phase ordinary refers are
eligible for macro resolution through that catalog, including renamed refers;
qualified ordinary aliases already follow the existing phase alias lookup.
Runtime ordinary-import inference remains unsupported and is not widened.

Focused99003 terminal0:21passed/0failed/0ignored across imports5/definitions3/
phase3/repl6/source4, /private/tmp/suss-pr129-review-after.log. Discovery/modules
7962 terminal0:14passed, /private/tmp/suss-pr129-review-modules.log. Independent
command probes exercised forward declarations, explicit macro-body dependencies,
duplicate alias failure, excluded defmacro and retry after Macro initializer
throw. Full19941 is live at /private/tmp/suss-pr129-review-full.log; source/tests
are frozen. Require terminal baseline and exact reviewed-head CI before readiness.
No merge or completed M3 claim; all previously recorded requirements remain open.


PR128 final reviewed3fa84b5 CI36941605461 completedSUCCESS:887passed/0failed/
17existing ignores/81groups, Python86, all six native command guards and final
reader doctests. Root verified /private/tmp/suss-pr128-final-ci.log, watch93300/
log45922 terminal0, then marked128ready:API OPEN/notdraft/headsame. No merge.

PR129 review-fix Python43118 terminal0:86passed. Provenance40015 terminal0:
132 core imports and1065 upstream declarations verified; diff whitespace passes.
Full19941 remains live; no terminal result or readiness is claimed yet.

## Source macro reload continuation

PR129 independently reviewed fix9256e6c4e99b70a1de0d6dc4ab8cd151a99bef5e pushes
significant alias/refer repairs: actual Macro exports are published after real
compiled definition success; Macro ordinary aliases/renamed refers now resolve
those roots during macro-body expansion. Focus21/module14/Python86/provenance pass.
Final-source full19941 is agent-scoped to review_pr129, which retains polling
ownership/heavy slot; root cannot write_stdin that agent's handle. Do not restart.
Root authoritative escalated ps confirms Cargo PID64246 live (~3m39s elapsed);
reviewer also re-polled19941 live at conformance. Log /private/tmp/suss-pr129-review-full.log.
Final-head CI36944259806 IN_PROGRESS on9256. No PR129 readiness before full/CI.

PR128 exact reviewed3fa84b5 CI36941605461 SUCCESS887/0/17/81/Python86/allnativeguards/
finalreader verified from /private/tmp/suss-pr128-final-ci.log (ANSI stripped for
reader labels); watch93300/log45922 terminal0. Root markedready; API OPEN/notdraft/
headsame. Nothing merged.

Root child /private/tmp/suss-m3-macro-reload portable/m3-macro-reload fast-forwarded
9256 and initialized real upstream pin. New compiled_macro_reload.rs has two
unexecuted native host regressions: metadata ^{:reload :reload} updates source
macro functions while previous Runtime expansions stay42; ^{:reload :reload-all}
updates reachable Macro helper source while plain reload preserves loaded helpers.
Syntax follows pinned analyzer reloads table (metadata :reload keyword value),
not invented :reload-all metadata keys. Wait for19941 terminal before focused
before-test; then implement actual phase reload/invalidation and preserve all
original M3 criteria (privacy/inference/AOT, full &form/&env/syntaxquote/gensyms,
versioned bounded bootstrap/cache/evaluator removal, #15 stackless cancellation/
pending I/O/cleanup/dynamic scope/live memory accounting). Goal remains active.

Source macro reload before84676 terminal101:both new host tests hit existing
located reload-metadata error; /private/tmp/suss-m3-macro-reload-before.log.
After86438 terminal101: incorrect reader enum spelling fixed to Kind::Bool;
after33571 terminal0:two new host tests pass. Implemented explicit MacroReload
Once/Reload/ReloadAll host policy, checked macro-only libspec metadata, protected
bootstrap core, actual selected phase identity invalidation after valid discovery,
reachable reload-all without losing unrelated provided units. Prior executing
Macro effects retain documented session behavior; failed definitions keep old roots
and Runtime input compile errors publish no bindings. Ordinary Runtime require
reload metadata/whole-clause flags remain diagnosed and unimplemented.

Added failure/retry, unrelated cached module and nil/false/invalid-policy/core
guards plus actual command metadata reload with once-only/captured-old behavior.
Focus88217 terminal0 before final guards; focus53125 terminal0 on fivehost guards.
Final source focus98700 now live, /private/tmp/suss-m3-macro-reload-final-focus2.log;
root owns heavy slot. Source/tests freeze after this final command addition.
Next module focus, Python/provenance and required full baseline before publication,
then mandatory independent review/significant fixes/exact-head CI. Full original
M3 bootstrap/environment/cache/phase-private/AOT and #15 acceptance stay required.
PR129 corrected reviewed9256 full19941 terminal0:895/0/17/83 through finalreader;
reviewer approved/no further findings, heavy slot released. CI36944259806 pending
exact9256. No readiness without CI and no merges.

Final source macro reload focus98700 terminal0:73pass/0fail/0ignore across10native
suites, /private/tmp/suss-m3-macro-reload-final-focus2.log. Module focus65355
terminal0:14pass (existing11/newphase3), /private/tmp/suss-m3-macro-reload-module-focus.log.
Source/tests frozen now; full19242 live, /private/tmp/suss-m3-macro-reload-full.log,
root owns exclusive heavy slot. Re-poll19242 to terminal; do not restart due to
observation timeout. Python83820 live, /private/tmp/suss-m3-macro-reload-python.log.
PR129 exact reviewed9256 CI watcher27467 live, /private/tmp/suss-pr129-final-ci-watch.log;
no readiness until exact final-head CI. Provenance/inventory1065/reviews224841/import132
verified, selection unchanged. Only docs/evidence may change during full. Next
full/Python pass then draft publication, independent review/significant fixes and
exact-head CI; preserve all original remaining M3 acceptance including real cache
keys/versioned bootstrap/evaluator removal and #15 stackless interruption/I/O/live
accounting. No source ABI/dependency changes/new skips and no merges.

Macro reload Python83820 terminal0:86tests pass. Full19242 confirmed live and
progressing through portable_dynamic_bindings; no terminal/full-success claim.
Issue13 progress comment5943209138 records focused reload evidence and remaining
M3 namespace/bootstrap/cache/lifecycle criteria. Source/tests remain frozen.

Source macro reload full19242 terminal0:901passed/0failed/17existing ignores/84groups through finalreader doctests, /private/tmp/suss-m3-macro-reload-full.log. Source/tests frozen, native73/module14/Python86/provenance verified. Heavy slot released. Publish draft and require independent review/significant fixes/exact final-head CI before readiness. M3 original requirements remain in progress.


PR131 independent review of710dbb6 found significant reload-metadata precedence
bug: prefixes are stored outermost first, but source policy validation treated
last prefix as effective. Pinned vendor/tools/reader.clj372–386 merges outer over
inner. New native outer-prefix regression before98106 terminal101:outer reload
with inner nil incorrectly retained42 instead of reading43;
/private/tmp/suss-pr131-metadata-before.log. Fixed policy lookup selects first
outer reload key and validates only effective value; tests cover nil/false masking
inner reload/invalid7/shorthand, outer reload masking inner invalid7, effective
invalid outer rejection, and ordinary Runtime reload diagnostics. Corrected
focus67370 terminal0:22pass (defs3/import5/reload7/repl6/independentprobe1),
/private/tmp/suss-pr131-review-after.log. Independent probe checks discovery
failure retains provided namespace and caller scope, nested explicit reload works,
and reachable cyclic reload-all fails preserving old Runtime expansions/scope;
/private/tmp/suss-pr131-independent-probe.rs (temporary test removed).
Python98961 terminal0:86, inventory1065/reviews224841/import132 verified. Module
focus88831 live; reviewer owns heavy slot. No readiness until corrected full and
exact final reviewed-head CI; no merge and original M3 requirements remain open.

PR131 corrected module focus88831 terminal0:14pass,
/private/tmp/suss-pr131-review-modules.log. Final-source full82782 is live,
/private/tmp/suss-pr131-review-full.log; source/tests are frozen, reviewer retains
polling ownership/heavy slot. Re-poll to terminal, never restart solely due to an
observation timeout. All provenance remains unchanged; this fix copies no source,
changes no runtime ABI/dependencies/selection and adds no skips.

PR131 significant review fix bcb1c5cfec33c48b62d7abafbbae574eb55e5de5 pushed:
outer-prefix effective policy lookup plus focused regression/provenance docs.
Final-source full82782 terminal0:902passed/0failed/17existing ignores/84groups
through finalreader doctests, /private/tmp/suss-pr131-review-full.log. Focus22,
module14/Python86/inventory1065/reviews224841/import132 pass. No further significant
review findings after independent nested reload/discovery failure/cycle scope
probes. Reviewer releases heavy slot; source/tests remain unchanged. This evidence
only docs commit must also receive exact final-head CI before readiness. No merge;
M3 bootstrap/cache/full environment/privacy/shared AOT and lifecycle remain open.

## M3 persistent macro vector continuation

PR129 exact reviewed9256e6c CI36944259806 SUCCESS:895/0/17/83groups through final
reader doctests, Python86/inventory1065/reviews224841/import132. Root downloaded
/private/tmp/suss-pr129-final-ci.log and marked129ready; no merge.
PR131 independently approved03a6d8f3bc79d09eea6a989b533dcc972f9f4393:
reviewer full82782 terminal0:902/0/17/84 through finalreader; final-head CI
36947152902 confirmed in_progress, not ready yet.

New branch portable/m3-macro-vectors at /private/tmp/suss-m3-macro-vectors starts
from reviewed03a6d8f and real initialized source pin. Retains complete vector type,
15 source supporting declarations, indexed traversal and standalone canonical
EMPTY setup with explicit provenance. Quoted vectors and canonical rooted vector
transport now execute compiled binding-vector macro results. Original Array.slice
adapter supplies real copied source storage for upstream pop. No removed source
methods or fake collection implementations. See docs/runtime/compiled-macro-vectors.md.

Before83126 terminal101 unresolved PersistentVector. Subsequent after81426/78480/
3861/58878/78131 exposed Error adaptation, munged EMPTY property, ILookup and
missing slice. After52574 and96266 terminal0 basic vector and macro expansion pass.
Focus6659 terminal101:1,100-element stress exhausted default10million fuel; explicit
bounded100million test allowance fixes that. Final-focus93439 terminal0:33tests
across six native suites. Python38758 terminal1 caught bad mock intercepting an
unrelated earlier patch; corrected95327 terminal0:87tests. Provenance147generated
files,7complete licensed setup forms. Runtime focus62999 terminal101:the UID
exhaustion test expects scalar allocator as final private global; moved Array.slice
root before allocator and adjusted internal constants. Corrected runtime focus
74850 is live at /private/tmp/suss-m3-macro-vectors-final-focus2.log; root owns
exclusive heavy slot. Re-poll to terminal. Do not claim full success yet.

Next unblocked implementation: retain and execute actual transient vector/
fromArray dependencies so large literal and quoted vector construction uses the
pinned algorithm; then metadata maps/&env/syntaxquote/versioned bootstrap/cache.
Current constructor/transport prerequisite must pass frozen full baseline and
independent PR review/exact-head CI before readiness. All original M3 issue12–15
acceptance remains required, including whole source privacy/cache/AOT and stackless
scheduler cancellation with pending-I/O cleanup/dynamic scope/live-GC accounting.

Corrected runtime focus74850 terminal0:61tests across vector5/portable arrays10/
CLIprint1/ABI45; scalar UID exhaustion and the shared slice root both pass.
Python95327 terminal0:87tests. Current inventory239partial/826unassessed,147core
import files,7complete standalone licensed forms. Source/tests frozen now; root
starts required full workspace baseline, exclusive heavy slot remains root-owned.
No milestone completion or full/remote/readiness claim until terminal evidence.

Full workspace11057 confirmed live and advancing through compiled macro/phase
native suites with no failures, /private/tmp/suss-m3-macro-vectors-full.log.
Root owns this process and exclusive heavy slot; poll11057 until terminal before
another Cargo/JVM/Node workload. Source/tests stay frozen. Partial milestone
progress recorded on issue14 comment5943556369; no issue closure. Final CI131
36947152902 remains confirmed in_progress on approved03a6d8f. Do not use older
head CI. No PR for vectors yet; obtain full and fresh differential evidence,
then independent review/significant fixes/final-head CI before readiness.

## M3 vector factory completion within the current increment

Full11057 terminal101 (not success): portable_collection_literals stale missing-
constructor test assumed imported PersistentVector remained absent. Kept the real
compile-atomic missing-constructor regression in a fresh Session with no core
import, rather than deleting/skipping it; corrected67415 terminal0:6tests.

Extended this same vector increment with complete retained transient protocols,
array-copy, edit-token helpers, TransientVector, transient/persistent!/conj! and
actual fromArray standalone factory. Factory boundary test52966 never executed:
os Permission denied while launching newly built test binary; no semantic before
result is claimed. Subsequent28262 terminal101 reproduced real transport error:
persistent! clears only root edit token; descendant nodes retain old tokens.
Decoder corrected to enforce nil root token and allow actual retained descendants.
After8527 terminal0:6native tests, including31/32/33/65/1057 quoted factory data,
copy-on-write and post-persistent! count/nth/conj/assoc/pop/persist errors.

Oracle initial65366 terminal1: wrong generator working directory meant namespace
was not generated. Corrected generator then8839 terminal1: main transport requires
generated base cases. Ran scripts/oracle_cases.py, then62738 terminal0 pinned
compiler, Node terminal0 and exact compare37passed. Native87954 terminal0:7vector
and6literal tests, including37 observations independently decoded in EACH of
Runtime and Macro Stores; no fake constructor fixture. Added active-root/narrow-
node malformed guards and exact nested signed-zero bits, preserving all37cases.
Python46936 terminal0:87tests. Source-backed inventory253partial/812unassessed;
161core import files and8complete standalone licensed setup forms verified.

Final source focus53158 is live at /private/tmp/suss-m3-vector-factory-final-focus.log;
root owns exclusive heavy slot. Includes vector7/literals6/forms6/indexed3/source4/
trie5/arrays10 and runtimeABI45/compilerarrays. Re-poll to terminal, then freeze
source/tests and run required full workspace baseline. PR131 reviewed03a6d8f CI
36947152902 confirmed live at01:08UTC, test step in progress. Watch21496 live.
No vector PR yet; require full then independent review/significant fixes/final-head
CI. Next unblocked original M3 task is persistent map/metadata transport and &env/
syntaxquote; complete bootstrap/cache/privacy/AOT/evaluator removal and #15
stackless scheduler/cancellation/pending-I/O/live-GC accounting remain required.

Final focus53158 terminal0:87Rust tests/9groups, including45ABI. Preserved original
37oracle cases and added fromArray alias/clone plus transient-pop trie boundary:
final pinned compile45348 terminal0, Node terminal0, exact compare40pass;
final native7103 terminal0:7tests, all40observations independently decoded in each
of Runtime and Macro phases. No expected observation was replaced. Source/tests
frozen now for the required full workspace baseline; root retains heavy slot.

PR131 exact reviewed03a6d8f CI36947152902 completedSUCCESS:902passed/0failed/
17existing ignores/84groups through finalreader, Python86/import132 and existing
provenance checks. Root verified /private/tmp/suss-pr131-final-ci.log, watch21496
terminal0, and marked131ready. No merge. New vector issue14 progress remains
partial until all original acceptance (including maps/&env/syntaxquote/cache/
bootstrap/evaluator and stackless lifecycle) is actually proved.

Final-source full42522 confirmed LIVE through vector7 and compiled_phase_session
(native suites), /private/tmp/suss-m3-vector-factory-full.log. Root owns process
and exclusive Cargo/JVM/Node slot; poll42522 to terminal, never restart solely
because observation times out. Source/tests remain frozen. Issue14 partial
factory progress comment5943730682 records40fresh primary observations and both
native phases, remaining acceptance and no completed milestone claim. PR131 API
verified OPEN/notdraft/head03a6d8f after approved-head CI SUCCESS. No merge.
After full terminal, publish vector draft with Refs#14, dispatch independent
subagent review, push significant fixes and require final-head CI. Next source
work is persistent maps/metadata/&env, preserving all other original M3 gates.

Vector full42522 terminal101, not success: old portable_quoted_identifiers guard
still expected '[1 2] to fail after real vectors became supported. Preserved that
row's compile-atomic metadata failure using '^:tag [1 2], while independent vector
success tests already cover plain/large quoted values. Focus88582 terminal0:
7vector+3identifier tests, retaining all48identifier and40vector primary cases.
No production code changed, no removed tests or skips. Commitb5fc263 holds frozen
vector source; next commit records the guard update, then required full rerun.
Root heavy slot free until full restart on this corrected source/test snapshot.
Child portable/m3-macro-maps /private/tmp/suss-m3-macro-maps currently has no edits
and real initialized pin; fast-forward it to the corrected parent before new work.

Vector final-source full80479 terminal0:909passed/0failed/17existing ignores/
85groups through finalreader doctests, /private/tmp/suss-m3-vector-factory-full2.log.
Final source709918b includes both preserved stale error guards; focused identifier/
vector10 and final primary40native-each-phase pass. Python87/provenance161/
inventory253partial812unassessed/setup8 verified. No new ignores. Heavy slot free;
publish draft, dispatch mandatory independent review, fix significant findings,
require exact final reviewed-head CI before readiness. No merge or M3 completion.
Child map worktree is separate: source-backed import preparation is unexecuted,
not passing evidence; use focused tests before any new full baseline there.

Map child now has an unexecuted focused regression for actual quoted canonical
PersistentArrayMap data, nested binding vectors, persistent assoc/dissoc,
lookup/defaults and order-independent equality/hash. No success is claimed; wait
for parent80479 full terminal before Cargo/JVM/Node. Parent source remains frozen
at709918b (vectorb5fc263 plus stale quote-guard fix), full /private/tmp/suss-m3-vector-factory-full2.log.

Map child focused83416 terminal101: retained MapEntry #js reader dispatch failed.
Explicit source-array patch fixed that; source8343 terminal101 exposed private
lookup-sentinel metadata, recorded bounded bootstrap omission;83527 terminal101
exposed when-some. Whole-form adaptation preserves initializer-once nil test.
Initial Rust15458 terminal101 was a local iterator-shadowing compile error, fixed.
Source87976 terminal0: actual canonical quoted map, persistent updates/equality/hash.
Provenance narrow threshold target normalization and negative regression pass;
Python69914 terminal0:88tests/import191/reviews283partial782unassessed/setup12.

Focus44596 terminal101: foreign-class test incorrectly called missing factory after
rebinding class. Direct foreign constructor preserves meaningful nominal rejection.
Focus56714 terminal0:22tests (maps3/forms6/vectors7/literals6). Fresh oracle91859
terminal0: pinned compile, Node, exact24tagged observations, native maps4 in EACH
Runtime and Macro phase with independent decode/GC. Final focus83893 terminal0:
5map tests adding actual factory capture, textual order and throw short circuit.
Logs /private/tmp/suss-m3-macro-maps-{before,source,after2,after3,focus,focus2,oracle,final-focus}.log.

Incidental cargo fmt touched70unrelated files. Automatic review rejected broad
restore; every file was backed up under /private/tmp/suss-map-incidental-format-backup
and proved byte-identical to rustfmt edition2024(HEAD). Reviewed retry restored
only those verified formatting files; actual map changes preserved. No blocker.
Next integrate parent739f0df publication evidence, freeze and run required full
workspace baseline; then mandatory independent review/significant fixes/exact-head
CI before PR readiness. No map PR yet. Retained HAMT/transients, metadata transport,
&env/syntaxquote/gensyms/bootstrap/cache/evaluator removal and stackless scheduler/
I/O cancellation/live-GC accounting remain original M3 acceptance, not completed.
PR132 exact739f0df CI36951307622 confirmed in_progress, not yet ready; no merge.

Parent739f0df integrated with both handoff evidence blocks preserved. Map source/tests
frozen for required full workspace baseline; root owns exclusive heavy slot.

Map draft PR133 published from a00cf9a with Refs#14; mandatory independent
review_pr133 dispatched. Required full75886 terminal101, not success: old
portable_quoted_identifiers quote-error row still expected supported '{:x 1}
to fail. Preserve compile-atomic metadata failure with '^:tag {:x 1}, retaining
all48identifier cases and actual plain map success coverage. Reviewer flagged
map sequence synthetic-entry nesting bound; owns heavy slot now to prove/fix/push
significant finding before corrected full. Root edits only this stale test guard
and handoff during review. PR132 exact739f0df CI36951307622 remains LIVE; root
watch26758 confirmed live, /private/tmp/suss-pr132-watch.log. No ready/merge claim.

PR133 independent reviewer confirmed and pushed significant nesting fix9f9e0c4:
synthetic map entry vectors now count their own level before decoding children.
Before61059 terminal101 accepted overdeep syntax; after33943 terminal0:6map+
3identifier. Additional bounds68037 terminal101 hit default fuel during fixture
construction;39582 terminal101 rejected correctly but diagnostic assertion was
over-specific. Final31032 terminal0 with explicit bounded fuel:6map+3identifier,
61/62wrapper boundary in bothStores, total4096node guard, malformed/oversized/
metadata/index rejection and recovery. Python88/import191/setup12 independent
checks pass; reviewer approves exact9f9e0c4813455a9c33a5269ddd5a7c4a5db5599e.

Corrected frozen full91421 terminal0:915passed/0failed/17existing ignores/86groups
through finalreader doctests; /private/tmp/suss-m3-macro-maps-full2.log. No new
ignores or removed tests. PR133 remains draft pending final reviewed-head CI.
This publication evidence changes docs only; tested source/tests remain9f9e0c4.

PR132 exact reviewed739f0df CI36951307622 completedSUCCESS:909passed/0failed/
17existing ignores/85groups, Python87/import161/setup8/reviews253partial812unassessed
through finalreader. Initial log assertion missed ANSI separators; stripping only
ANSI display codes confirmed full evidence. Root verified /private/tmp/suss-pr132-final-ci.log
and watch26758 terminal0, then marked132ready; API OPEN/notdraft/head unchanged.
No merge. Issue14 progress5944079840/5944158852 records partial map/review proof.
Metadata child is separate from frozen map source; its first focused68659 terminal101
shows quoted metadata rejection, /private/tmp/suss-m3-macro-metadata-before.log.
Next unblocked work is real metadata transport and &env/syntaxquote; HAMT/transients,
bootstrap/cache/privacy/evaluator removal and stackless scheduler/I/O cancellation/
live-GC accounting remain required original M3 acceptance. No completion claim.


## Open PR stack ancestry repair — 2026-10-01

The requested pr-stack-fixes session found PR #129 conflicting with current main
8ef2f07 after predecessors #122–#128 were squash-merged. Resolved the overlap by
retaining the independently reviewed macro-import changes and integrating main
ancestry. Propagated ancestry merge commits through #131/#132/#133 without
rewriting their original commits. Before this evidence-only addition, every
repaired tree exactly matched its corresponding reviewed original tree:

- #129: 9256e6c -> 39b3e9f; original parent tree also exactly matches main.
- #131: 03a6d8f -> 54e2402; repaired #129 is an ancestor.
- #132: 739f0df -> df4661c; repaired #131 is an ancestor.
- #133: 6d97fa1 -> 2a12919; repaired #132 is an ancestor.

Separate independent subagent reviews for each PR found no significant issue.
No production semantics, source/license hashes, tests, issue IDs or inventory
statuses changed. All source/tests were frozen during verification.

Commands and terminal results (CARGO_BUILD_JOBS=2, no RUSTFLAGS override):

- Focused cargo test -p suss-cli --test compiled_macro_imports --test compiled_macro_reload --test compiled_macro_vectors --test compiled_macro_maps --test portable_quoted_identifiers --locked -- --test-threads=2: 28 passed, zero failed/ignored; /private/tmp/suss-pr-stack-fixes-focused-shared.log.
- cargo test --workspace --locked -- --test-threads=2: 915 passed, zero failed, 17 existing ignores across 86 groups, exit 0 through reader doctests; /private/tmp/suss-pr-stack-fixes-full.log.
- python3 -m unittest discover -s scripts -p 'test_*.py': 88 passed, exit 0; /private/tmp/suss-pr-stack-fixes-python.log.
- Inventory/review/import checks: 1,065 declarations, 283 partially reviewed, 782 unassessed; 191 core import files and 12 complete licensed setup forms verified. Sequence/bitwise provenance, all 15 WIT files/six packages, numeric artifact integrity, offline roadmap preview and git diff --check pass.

The initial fresh-target Cargo build was deliberately terminated (exit 143)
before execution to reuse the existing repository cache. Focused and full runs
then executed sequentially with CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target;
no overlapping native Cargo invocation or skipped regression.

Published #129–#132 repairs have new exact-head CI runs 36956034013,
36956034110 and 36956034518, pending at this evidence snapshot. Superseded
36956033925/36956033885 are cancelled, not passing. #133's original reviewed
6d97fa1 passed 36953923303, which does not certify its ancestry repair. Require
new successful exact-final-head CI for all repaired PRs before readiness.
Keep #133 draft until its repaired head passes. No PR was merged.

Refs #13/#14 remain partial; M3 and subsequent milestone acceptance remain open.
Next unblocked implementation is metadata transport and &env/syntaxquote in the
separate child increment; full HAMT/transients, bootstrap/cache/privacy/evaluator
removal and stackless scheduler/I/O cancellation/live-GC accounting remain required.
Metadata child /private/tmp/suss-m3-macro-metadata at reviewed map9f9e0c4 now
contains an unexecuted regression for nested real metadata maps, original explicit
:line metadata on transported forms, both Stores and forced GC. Parent corrected
full91421 is confirmed LIVE and root owns heavy slot. Do not run child Cargo yet.
Pinned compiler elides source/compiler metadata for ordinary runtime literals,
while form transport must preserve explicit reader data for &form; keep these
purposes separate within the shared quote lowering instead of a phase-dependent
semantic change. No metadata success or completed M3 acceptance is claimed.

Map corrected full91421 terminal0:915/0/17/86groups finalreader. Parent final docs-only
6d97fa10078f6088a31b3003dacceb84cbc641fa independently approved by reviewer133;
source/tests unchanged from9f9e0c4. Push79327 terminal0; new exact-head CI36953923303
confirmed LIVE. PR132 final reviewed739f0df CI36951307622 SUCCESS verified complete
909/0/17/85groups/Python87/import161/setup8/finalreader after stripping ANSI display
codes; root marked132ready, no merge. Integrate final map docs before child publish.

Metadata before68659 terminal101: quoted metadata rejected. After74196 terminal0:
1nested canonical transport test bothStores/GC. After16523 terminal0:3metadata tests
for prefix precedence/runtime reader-key elision, returned sequence metadata,
malformed/foreign metadata and cycles. Shared quote lowering now has explicit
native form-data purpose via suss.bootstrap/quote-form; ordinary literals elide
pinned irrelevant keys, native transport retains explicit data for &form.

Metadata focus44822 terminal101: stale compiled_macro_forms expected valid private
metadata to fail; preserve located compile-atomic error with a manually malformed
Number metadata form. Quote-identifier error rows use invalid ^42 prefixes on the
same symbol/vector/map targets. Focus79611 terminal101 was case-sensitive diagnostic
assertion; explicit Invalid metadata diagnostic corrected without dropping guards.
Oracle68237 terminal1 wrong plural runner namespace, corrected79531: pinned compile,
Node and exact24 observations pass, native terminal101 at vector-value-equality
with a generic language failure. Later source inspection identified the missing
RangedIterator js-mod binding; keep the case and retain its complete dependency.

Seven complete chunk declarations prepared; initial core_import rejected changed
forward-loader hash. Focus97220 was confirmed LIVE compiling stale artifact, so
identified owned CargoPID17814 and deliberately SIGINT stopped it; terminal130,
not success. Updated exact loader input hash and regenerated198licensed artifacts.
Reviews290partial775unassessed; setup12 unchanged. Correct-artifact focus47311 LIVE
/private/tmp/suss-m3-macro-metadata-focus4.log; root owns exclusive heavy slot and
source/tests frozen until terminal. Independent Python53681 terminal0:88tests.
Next poll47311 to terminal, repair only evidenced failures, then rerun fresh24
primary/native comparison and complete focus before full baseline/review/CI.
No metadata PR or M3 completion claim. HAMT/transients, &env/syntaxquote/gensyms,
bootstrap/cache/privacy/evaluator and stackless scheduler/I/O/live-GC remain open.


Metadata child validation update: focus47311 terminal101 retained vector equality
failure after real chunk declarations; source RangedIterator calls missing js-mod.
Retained the complete pinned js-mod wrapper and added original binary64 remainder
adapter, with exact scaling/subtraction, scalar coercion and signed-zero/NaN/edge
handling. Generator now retains199 licensed artifacts (195 source selections),
setup12. Diagnostic probe89443 failed in its own exception .message lookup and
establishes no semantic result; moved outside the tracked suite.

Focused94341 terminal0:27 tests. Fresh oracle55107 terminal0:45 pinned compiler/Node
observations match native decoding in BOTH Runtime and Macro stores after GC.
Final focused73950 terminal0:29 tests across metadata7/forms6/maps6/vectors7/ids3,
including actual chunk backing corruption, core-callee capture and remainder
arity/evaluation-order recovery. ABI/bitwise66784 terminal0:48 tests (45+3).
Logs: /private/tmp/suss-m3-macro-metadata-oracle3.log,
/private/tmp/suss-m3-macro-metadata-final-focus.log,
/private/tmp/suss-m3-macro-metadata-abi-focus.log. Full workspace baseline and
mandatory independent review/final-head CI still required before ready.

Concurrent authorized stack repair changed map parent to3ccd3723 and vector to
df4661ca. Map source/tests equal reviewed6d97fa1; only43 handoff lines differ.
Independent reviewer133 approved exact3ccd; CI36957307954 remains LIVE. PR133 was
restored to draft when changed head was discovered. PR132 exactdf4661ca fresh
CI36956034518 SUCCESS. Integrate final map ancestry/evidence into metadata child
before publishing; never overwrite repaired parent branches with old local heads.
Original M3 issues12–15 remain open; this prerequisite does not fulfill their
complete acceptance criteria.

Metadata commit rebased onto final repaired map3ccd3723; preserved both parent
stack-repair and child validation evidence. Production/tests unchanged by rebase.
Final independent Python suite:88 passed; inventory1065/reviews291partial774unassessed/
import199/setup12 verified; sequence/bitwise/WIT/numeric/offline checks pass.
Next run the required full workspace baseline on this frozen metadata tree.


## PR135 independent review and significant fixes — 2026-10-01

Root pre-review full97215 terminal0:922passed/0failed/17existing ignores/87groups
through finalreader doctests; /private/tmp/suss-m3-macro-metadata-full.log. This
baseline precedes the review fixes below and does not prove their final head.

Independent review of83a92e5 found two significant metadata issues. First,
ordinary vector/map/set metadata was quoted instead of analyzed in its lexical
environment. Before93981 terminal101 reproduced lexical answer7 becoming a symbol,
/private/tmp/suss-pr135-review-before.log. Pinned analyzer analyze-wrap-meta uses
analyze-map; quoted data and empty-list constants use constant emission instead.
Reader-prefix coalescing now happens before analysis, preserves the inner existing
key object and the outer winning value, ignores spans/metadata in reader-data key
equivalence, and orders surviving entries textually under the accepted variance.
Discarded expressions never run. Runtime map construction/association still owns
equality of evaluated runtime keys. with-meta callee-first capture is preserved.

Second, quoted/native metadata constructed discarded inner values before overwriting
them. Rebinding PersistentVector.fromArray exposed that construction. Initial
fixture45348 terminal0 used a tiny vector's direct constructor and did not exercise
the factory; corrected32-element fixture43825 terminal101 proves the bug,
/private/tmp/suss-pr135-review-data-before2.log. Shared bounded reader-prefix
coalescing now removes discarded data before construction in quote/native paths.
Function literal metadata remains required subsequent work, explicitly documented.

Reviewer exclusive heavy-slot commands/results, all sequential with shared target,
CARGO_BUILD_JOBS=2 and no RUSTFLAGS override:
- Focus38821 terminal0:30 tests/5groups after the ordinary-expression fix.
- Fresh oracle9220 compile terminal0 and exact53primary observations; oracle91626
  terminal0:55fresh primary observations plus8native metadata tests, bothStores.
- Final focused60479 terminal0:31 tests/5groups, metadata9/forms6/maps6/vectors7/ids3;
  /private/tmp/suss-pr135-review-final-focused.log.
- Final scripts/test-macro-metadata-oracle.sh75480 terminal0:56fresh pinned compiler/
  Node observations match exact expected tags/bits; native9metadata tests independently
  execute all56cases in BOTH Runtime and Macro Stores after GC;
  /private/tmp/suss-pr135-review-final-oracle.log. Original45cases remain unchanged.
  Pinned bool/string js-mod warnings remain visible; no skips or expectation rewrites.
- python3 -m unittest discover -s scripts -p 'test_*.py':88passed, terminal0,
  /private/tmp/suss-pr135-review-python.log. Import199/setup12, inventory1065 and
  reviews291partial774unassessed independently verified; git diff --check passes.

Significant fixes must be pushed to PR135; then require a new frozen full baseline
and successful exact-reviewed-final-head CI before readiness. No PR was merged.
M3 issues12–15 stay open; function metadata, HAMT/transients, &env/source locations/
syntaxquote/gensyms/bootstrap/cache/evaluator removal and stackless scheduling/
I/O cancellation/live-GC accounting remain original required acceptance.


PR135 follow-up review identified duplicate structural-key normalization gaps in
5b46a6d. Suss reader/native Forms can retain repeated map/set syntax entries;
comparison must use canonical last-value maps and unique set members rather than
raw length or reusing the same member match. Initial duplicate regression66629
terminal0 reused a stale shared-target artifact from the isolated callable child,
not the current source; touching current quotes/hir forced a rebuild. Correct
before57189 terminal101 proves broken duplicate-map prefix precedence,
/private/tmp/suss-pr135-review-duplicate-before2.log. Added both directions of
repeated-map versus distinct-map keys and actual native quote/read coverage.

Canonical map/set comparison now normalizes duplicates under the existing depth/
work bound, preserving the first key object and final map value. Unit9887 compile101
was explicit Vec<&Form> inference, corrected28032 terminal0:1 focused semantic
unit for both-direction duplicate/equal/unequal/nested keys and exhausted bounds,
/private/tmp/suss-pr135-review-duplicate-unit2.log. Correct-source focus10133
terminal0 explicitly rebuilt reader/compile/CLI:32tests/5groups (metadata10/forms6/
maps6/vectors7/identifiers3), /private/tmp/suss-pr135-review-duplicate-focus.log.
Fresh scripts/test-macro-metadata-oracle.sh42954 terminal0:unchanged56primary
observations match exactly plus10native metadata tests in bothStores/GC,
/private/tmp/suss-pr135-review-duplicate-oracle.log. Python88/import199/setup12/
inventory1065/reviews291partial774unassessed and diffcheck pass. No primary duplicate
textual-reader claim; native Form cases preserve existing accepted transport data.

Push this significant follow-up fix, freeze its source, then require full workspace
baseline and successful exact-final-head CI. Previous83a92e5/5b46a6d CI does not
certify this new source. No merge; original M3 acceptance remains open. Root owns
next full baseline after reviewer releases the exclusive heavy slot.


PR135 reviewed source2fe68294165d0f5610b1f954c9253a4f1067697b frozen full74540
terminal0:926passed/0failed/17existingignores/87groups through finalreader doctests,
/private/tmp/suss-m3-macro-metadata-reviewed-full.log. Reviewer135 independently
approved exact2fe after pushing all3 significant findings. This final publication
evidence changes docs only; require independent final-head confirmation and fresh
exact-head CI, keeping draft until terminal success. No merge.

PR133 exact repaired3ccd3723 CI36957307954 SUCCESS:downloaded complete
/private/tmp/suss-pr133-repaired-final-ci.log, stripped only ANSI display codes,
verified915/0/17/86groups/Python88/import191/setup12/finalreader. Reviewer133
independently approved3ccd. Root marked133ready and verified OPEN/notdraft/head
unchanged; no merge. Repaired #12939b3e9f CI36956034013, #13154e2402 CI36956034110,
#132df4661c CI36956034518 also SUCCESS on their new heads. Old-head/cancelled runs
were not reused as repair proof.

Next child /private/tmp/suss-m3-callable-metadata integrated reviewed2fe and has
three meaningful BEFORE94209 terminal101 regressions for ordinary MetaFn/collection
calls and function-literal metadata. Initial original HIR/ABI adaptation is prepared
but unexecuted: source callee captured once; closures retain universal invocation;
nominal callable method captured before arguments through canonical IFn key;
actual retained source methods own behavior. Native explicit -invoke remains its
separate path. No new core stub or fake MetaFn. ROOT owns next focused child run.
Full M3 issues12–15 remain open, including source environments/locations/syntaxquote/
gensyms/bootstrap/cache/evaluator removal and stackless lifecycle acceptance.
Next callable-metadata prerequisite prepared in isolated child
/private/tmp/suss-m3-callable-metadata, branch portable/m3-callable-metadata,
currently based on pre-review metadata83a92e5. Three focused regressions are
UNEXECUTED: ordinary MetaFn invocation preserving captured functions and metadata
across GC/redefinitions; lexical function-literal metadata with callable body;
canonical vector/map IFn invocation. No success claim or PR yet. Parent reviewer135
owns exclusive heavy slot for significant metadata fixes and fresh oracle; root
must integrate its pushed final source before executing this child.

Source inspection: current ABI invoke accepts canonical closures only. Complete
licensed MetaFn/IFn are already retained; explicit -invoke has a pin-specific
receiver convention and is not an implementation of ordinary callable-object
syntax. Generic calls need canonical IFn dispatch with one evaluated receiver,
correct implicit receiver arity, live methods and argument effects, plus ordinary
function literal metadata lowering. Preserve compiled protocol phase identity,
rooted old captures and shared ABI; do not use an evaluator or eager fake wrapper.
Full original M3 bootstrap/environment/cache/lifecycle obligations remain open.


Callable child before94209 terminal101:all3 regressions fail (0passed/3failed),
/private/tmp/suss-m3-callable-metadata-before.log. Source83a92e5 cannot ordinarily
invoke canonical MetaFn/vector/map values and ignores function literal metadata.
Integrated final reviewed metadata2fe68294165d0f5610b1f954c9253a4f1067697b into
child with both handoff blocks preserved. Parent full74540 confirmed LIVE,
/private/tmp/suss-m3-macro-metadata-reviewed-full.log; ROOT exclusive heavy owner.
No child Cargo/JVM/Node until parent baseline terminal. Next implement ordinary
callable dispatch and function metadata through source HIR/IR/shared ABI, retain
actual licensed MetaFn methods, then validate pin/native effects/arity/GC.


Callable child initial original source implementation now executes: callee once,
lazy closure/direct nominal IFn selection; actual method captured before arguments,
retained MetaFn source remains authoritative; lexical fn-literal metadata attaches
actual persistent maps. New private callable-bind export, unchanged ABI2 layouts.
Focus5261 terminal101 generated-Wasm type mismatch from double Boolean conversion;
corrected65606 terminal0:3callable+4identity regressions. New direct strict wrong-arity
regression added. Final focus71765 terminal0:18 tests (callable4/identity4/metadata10).

Fresh primary38248 terminal1 IFn extend-type probe required grouped signatures;
corrected44107 terminal1 exact comparison showed pin ignores vector extra args,
returning1. Retained that actual reference result; native explicitly tests accepted
strict-arity contract (effects111 then language error/catch111), not a skip or match.
Oracle78447 terminal0:67 fresh primary observations exact; native66matching+1explicit
strict boundary EACH Runtime/Macro Store after GC, all10metadata tests pass.
Logs /private/tmp/suss-m3-callable-metadata-focus2.log,
/private/tmp/suss-m3-callable-metadata-final-focus.log,
/private/tmp/suss-m3-callable-metadata-oracle3.log. ABI98764 confirmed LIVE for new
copied/corrupt callback/GC/captured-method guards plus full ABI regressions;
/private/tmp/suss-m3-callable-metadata-abi-focus.log. ROOT owns heavy slot.

Metadata135 parent final doc-only17f00a3d993695b564328c31b9c749e9bb9389b0 independently
approved; source/tests unchanged from reviewed2fe full74540 terminal0:926/0/17/87groups.
CI36960926070 confirmed LIVE on17f, draft. Parent133 independently reviewed3ccd fresh
CI36957307954 SUCCESS verified915/0/17/86groups/Python88/import191/setup12/finalreader;
markedready OPEN/notdraft/headunchanged, no merge. Integrate final metadata docs
before callable child publication. M3 remains open; next unblocked work after this
validation/review is source apply, environments/locations/syntaxquote/bootstrap/cache
and stackless lifecycle, preserving every original issue12–15 acceptance criterion.


ABI98764 terminal101:45pass/1failed; new fixture attempted to replace method with
nil, correctly rejected by protocol-method-set. Correct fixture uses a valid
arity2 callback replacement; new bound callback errors, old captured arity1 method
still works after GC. ABI92753 terminal0:46tests including malformed key/copyENV/
exception-tag/recovery guard, /private/tmp/suss-m3-callable-metadata-abi-focus2.log.
Source/core behavior unchanged by fixture repair. Import199/setup12/reviews291partial
774unassessed remain partial; current artifacts regenerated after evidence overlay.
Next integrate final metadata17f docs, freeze source and run full baseline, publish
Refs#14 draft and dispatch required independent review; no ready/merge claim yet.


PR136 published draft, Refs#14, source5367d075b34f7da69fc89b1ce4c4039cc6a428f0.
Independent reviewer136 approved exact5367 with no significant findings; import199,
review-overlay291/774 and licensed setup12 checks independently pass. ROOT full7750
terminal0:931passed/0failed/17existingignores/88groups through finalreader doctests,
/private/tmp/suss-m3-callable-metadata-full.log. Heavy slot released. This publication
changes docs only; require independent final-head confirmation and exact-final-head
CI before marking ready. No merge.

PR135 final17f00a3 independently approved, CI36960926070 SUCCESS. Downloaded complete
/private/tmp/suss-pr135-final-ci.log and verified926/0/17/87groups/Python88/import199/
setup12/finalreader. Marked ready after exact-head verification; no merge. Original
M3 issues12–15 remain open. Next unblocked task: retain complete source apply and
its callable behavior before macro environment/syntaxquote/bootstrap/cache work.


## Stack #137 rebase — 2026-10-02

At the user's request, imported GitHub stack137 using `gh stack checkout 137`,
rebased PR133 -> PR135 -> PR136 onto main03125fa using `gh stack rebase`, and
continued the cascading rebase with `gh stack rebase --continue`. Existing clean
map/metadata worktrees were temporarily detached and restored to their updated
branches. Initial checkout/rebase attempts reported occupied worktrees; no user
changes were overwritten.

The first map conflict attempted to replay quoted-identifier and later prerequisite
commits already squash-merged into main. Independently verified main03125fa, old
vector prerequisite739f0df and repaired prerequisite df4661c have identical complete
tracked tree b3199bec7a79a7684b27af8208162131b90c678b; first map commit a00cf9a
directly follows739f0df. Omitted only those sixteen duplicate prerequisites from
the active rebase plan, preserving all map/metadata/callable commits. Automatic
approval review initially rejected omission due to possible lost work; after
exact tree/boundary checks, the verified operation was approved.

Before this handoff-only addition, each rebased branch tree exactly equals its
original remote head: PR133 3ccd372 -> c05b212; PR135 17f00a3 -> 4e02022;
PR136 35d270f -> d6b8b1c. Separate independent reviewers approved each exact
rebased head, unchanged range-diff patches and new ancestry, with no findings.
No compiler/runtime/test/inventory/provenance content changed.

Validation on the frozen rebased top source, shared existing CARGO_TARGET_DIR and
CARGO_BUILD_JOBS=2, no RUSTFLAGS override:

- `cargo test -p suss-cli --test compiled_macro_maps --test compiled_macro_metadata --test compiled_macro_vectors --test portable_callable_metadata --test portable_quoted_identifiers --locked -- --test-threads=2`: exit0, 30passed/0failed/0ignored; /private/tmp/suss-stack137-focused.log.
- `cargo test --workspace --locked -- --test-threads=2`: exit0 through final reader doctests, 931passed/0failed/17existingignores/88groups; /private/tmp/suss-stack137-full.log.
- `python3 -m unittest discover -s scripts -p 'test_*.py'`: exit0, 88passed; /private/tmp/suss-stack137-python.log.
- CI inventory/review/core-import/sequence-provenance/bitwise-provenance/WIT-lock/numeric-runtime checks and offline roadmap preview: all exit0; /private/tmp/suss-stack137-checks.log. Stable issue IDs and existing acceptance statuses preserved.
- `git diff --check`: passed. No fresh JVM/Node oracle rerun; tracked source and
  fixtures are unchanged, and executing native baseline passed.

Push the updated branches with `gh stack push`, then verify remote exact heads and
fresh final-head CI. CI remains pending at this recording; earlier CI is not proof
for new commit IDs. No PR merge, issue closure or milestone completion is claimed.
Next unblocked development task remains complete retained source apply/callable
behavior before macro environments/locations/syntaxquote/bootstrap/cache and
stackless lifecycle work; original M3 acceptance remains open.

Next isolated worktree /private/tmp/suss-m3-source-apply, branch
portable/m3-source-apply, based on final reviewed callable35d270f. Real pinned
ClojureScript submodule initialized at c4295f303100bbf5afac449242d30bca1126f1a1.
Source apply BEFORE80197 terminal101:3failed/0passed, log
/private/tmp/suss-m3-source-apply-before.log. Regressions cover fixed/variadic and
captured functions/GC, actual MetaFn/vector/map/keyword calls, ordered operand
effects and language arity failure/recovery. These are intentionally unfinished
acceptance tests, not skips or baseline success; no PR/full-baseline claim yet.

Complete pinned bounded-count/spread/list*/next* now retained with exact extracted
source/EPL and explicit defn bootstrap, four new reviewed declarations. Import203
artifacts/reviews295partial+770unassessed; original core algorithms/branches retained.
Helper BEFORE43207 terminal101:2failed/0passed using unchanged parent core artifact;
focus66913 terminal101 rejected next*'s preserved :private attribute. Pinned
analyzer.cljc resolve* (1249ff) allows qualified private access with a warning.
Definitions now preserve that attribute and remain callable; warning emission and
namespace filtering remain unfinished (do not claim full privacy policy).
Focus43396 terminal0:3passed; fresh primary76888 terminal0:14exact observations and
private-access WARNING preserved, then3native tests. Final native70782 terminal0:
4passed including all14corpus cases after GC and bounded custom INext effect counts.
Logs /private/tmp/suss-m3-apply-sequences-{before,focus,focus2,oracle,final-focus}.log.
Inventory1065/review/import/diff checks pass. Only trailing whitespace in the new
spread patch was normalized after finalfocus; no semantic changes.

Next: retain complete apply-to/apply-to-simple/apply plus bounded reproducible
gen-apply-to/gen-apply-to-simple bootstrap. Existing named closure properties
explicitly reject call/apply; universal closure min/max bounds do not expose full
signatures/maxFixedArity/applyTo. Adapt the actual compiler-owned signature data
and source-array call/apply operations, keeping all pinned dispatch branches and
sequence forcing. Do not replace source apply with a host evaluator/sequence loop,
drop variadic/lazy branches, or mark the failing source apply regressions passing.
Need fresh primary/native callable/arity/effect corpus, required full baseline,
independent PR review/significant fixes and exact-final-head CI before readiness.
M3 issues12–15 remain open, including environments/syntaxquote/bootstrap/cache and
stackless lifecycle. PR136 final35d270f review independently approved, CI36963299893
confirmed IN_PROGRESS; keep draft until actual final-head success. No merge.


Source apply implementation in portable/m3-source-apply now retains full apply-to-
simple/apply source and every branch, plus bounded v1 pinned generator expansion
with complete licensed four generators/cs extraction and exact provenance checks.
Compiler creates rooted arity properties and compiled sequence-preserving applyTo;
native closure/IFn call/apply and source-array push preserve canonical phase keys,
large-argument tail packing, receiver identity, effects and language errors. ABI2
layouts unchanged. Stacker0.1.22 already locked, now direct compiler dependency;
bounded growth plus64-depth guard retains excessive-expansion diagnostics.

Execution logs in /private/tmp/suss-m3-source-apply-*:
- signatures-focus78739 terminal101 Number constructor expectedf64, corrected52260
  terminal0 existing8helper/callable tests.
- focus44869 terminal101 early bootstrap apply-to lacked-first declaration. Moved
  definition into the compiled apply-to-simple initializer after protocol declarations,
  retaining before forward declaration. focus39304 terminal101 depth24 exceeded.
- focus59307 terminal101 SIGABRT stack overflow after raising depth alone. Correct
  bounded stack growth23998 terminal101:controls3pass, apply2pass/1missingIFn call
  adaptation. No failed or aborted run counted as success.
- focus32598 terminal101 uncovered new nominal match omission; corrected34263
  confirmed live excessive JIT code (3.3GBRSS/400%CPU). Intentionally SIGINT stopped
  terminal130 after sharing keys investigation. Key table now hoisted once per
  phase/protocol;53733 terminal101 missingBTreeMap import, corrected26114 terminal101:
  apply3pass/1high-arity failure requiringsourceArray.push.
- focus6946 terminal0:4applytests/4.2s including25args andunboundedseqforcing.
- freshoracle82176 terminal1 observedvectorwrongaritypin1, retained actual primary
  value and added explicit strictnative47 boundary. corrected50421 terminal0:
  22freshprimary observations;5native tests eachphase20matches+2strict boundaries.
  original strictfixedpin7/native111 also retained explicitly.
- finalfocus98269 terminal0:30tests metadata10/helper4/callable4/control3/named4/apply5.
- ABI guard26367 terminal0:1new copied callback/emptypush/typedtag/recovery regression.
  Python54922 terminal0:88testsOK.
- closure/ABI32966 terminal101:13closurepass/1fixtureunresolvedseq. Updated only
  verifier setup to forwarddeclare required real seq dependency.13042 terminal101:
  all14closurepass; ABI executable NEVER EXECUTED because shared target disappeared
  between binaries. Root inspected other live Cargo/rustc processes; no new heavy
  run until shared environment is available. Do not claim47ABI or full success.
- inventory1065/import205/reviews301partial+764unassessed/bootstrapv1/setup12/bitwise/
  WIT15files6pkgs/numeric/offlinechecks terminal0 (38164), diffclean.

Concurrent external stack137 repair changed parent PR136 from35d270f to2b857d9,
only42handoff lines differ, source/tests identical. CI36963299893 CANCELLED; new
36965789944 confirmedLIVE. Independent reviewer136 approvedexact2b857d9 and verified
externalstackfull931/0/17/88. PR135 repaired4e02022 treeidenticalapproved17f; reviewer135
approvedexact4e. Root returned135todraft because newCI36965790098LIVE. No merges.
Next integrate repaired2b before publishing thischild, require47ABI and fullworkspace
baseline, mandatory independent review with significantfixes pushed and exactheadCI.
Docs/runtime/source-apply.md recordsscope/variance/failures/remainingoriginalM3gates.

PR138 published draft at52b4b0e on repaired parent2b857d9. Independent reviewer
review_pr138 approved source with no significant findings; inspected signatures,
named self/recur, sequence forcing, IFn tail packing and callback guards; bootstrap
provenance and diff checks passed. CI36967095236 terminalFAILURE: compiled macro
definitions test still expected :private rejection, contradicting retained pinned
metadata behavior. Exact log /private/tmp/suss-pr138-ci52b-failed.log. Updated test
to define and execute private-one=1 while keeping unsupported :const rejection,
failed redefinition and missing-body checks. Reviewer independently approved this
test-only correction; no skipped failures or broader privacy acceptance claim.
Fresh target rebuild focused command (CARGO_TARGET_DIR main target, jobs2, cargo
test -p suss-cli --test compiled_macro_definitions --locked -- --test-threads=2)
running session43968, log /private/tmp/suss-m3-source-apply-private-focus.log.
Required full workspace and47ABI remain unexecuted; exact final-head CI pending
new push. PR135/136 exact-head checks confirmedLIVE. No merges, issue closures or
M3 completion. Next run focused to terminal, full required baseline, resolve any
failures, re-review final head and verify final-head CI before readiness; then
continue complete macro environments/locations/syntaxquote/bootstrap/cache.

Source apply focused private-declaration suite43968 terminal0:3passed/0failed,
log /private/tmp/suss-m3-source-apply-private-focus.log; exact0482e6d independently
approved. Full workspace10188 terminal101 at portable_murmur_hash: old test
expected32 expanded/40 explicit nesting to fail, but guarded analysis now64.
Log /private/tmp/suss-m3-source-apply-full.log is failed baseline evidence, not
success. Rejection now uses two40-step thread macros (shallow source, combined
expansion beyond64); original16+16 and explicit40 forms retained as executing
positive33/41 checks. Existing per-macro256/255 budget, located diagnostic, no
publication and recovery checks preserved. Independent reviewer138 approved
test-only correction. Focus37039 terminal0:5passed/0failed/0ignored, log
/private/tmp/suss-m3-source-apply-limit-focus.log. Need push and fresh full baseline,
final-head independent review confirmation and exact-head CI before readiness.

Next M3 prerequisite isolated branch portable/m3-hash-map-data in
/private/tmp/suss-m3-hash-map-data, real pinned upstream initialized. Four new
not-yet-passing acceptance regressions cover9/16/17/32/33/65 map transport both
phases afterGC, persistent old roots/nil/equality/hash,20colliding keys, transient
array-to-hash growth/closed lifecycle. Development primary35observations fresh
compile/Node/compare terminal0: all exact. Initial runner27004 needed common
oracle fixture generation; corrected primary3 log and out observations retained.
Initial80405 failed wrong-directory setup, no primary evidence from that attempt.
Native BEFORE has just started; no map implementation or success claim yet.
Real HAMT/transient source needed before unrestricted &env maps; analyzer also
needs rich locals/namespaces and context semantics (pinned do final expressions
can become:return independent of current tail flag). &env/fullM3 remain open.

Hash-map BEFORE68971 terminal101:0passed/4failed/0ignored, log
/private/tmp/suss-m3-hash-map-before.log. Nine-entry quoted map reaches Language
failure; public dissoc/assoc! are unresolved before other tests execute. These
failures establish missing prerequisites, not successful HAMT evidence. Fresh
primary23130 compile terminal0, Node/compare terminal0:35exact pinned observations.
All unimplemented tests remain in isolated next worktree, outside PR138 baseline.

PR135 repaired4e02022 and PR136 repaired2b857d9 final-head CI completedSUCCESS:
36965790098 verified926passed/0failed/17existingignores/87groups;36965789944 verified
931passed/0failed/17existingignores/88groups. Both Python88/import199/setup12/runtime
ABI/final reader doctests present in complete logs
/private/tmp/suss-pr{135,136}-repaired-final-ci.log. Independent reviewers approved
exact heads previously; remoteheads rechecked unchanged. Marked ready, no merges.
PR138 e661139 independently approved; fresh CI36968779862 confirmedLIVE. Corrected
one handoff session typo above; final full workspace rerun follows this doc head.

Source apply required full75027 terminal0 at independently reviewed48c4b08:
CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2
cargo test --workspace --locked -- --test-threads=2. Verified941passed/0failed/
17existingignores/90groups through final reader doctests, including complete47ABI
with new foreignENV/emptypush callback regression. Log
/private/tmp/suss-m3-source-apply-full2.log. No new ignores or suppressed failures.
Source/tests unchanged by this documentation-only evidence update; final-head
independent review reconfirmation and fresh exact-head CI still required before
PR138 readiness. Prior48 CI36968929794 confirmedLIVE, not yet success. No merges
or issue/milestone closures; next unblocked work is complete retained HAMT/
transient source and canonical macro data, then original macroenvironment/
location/syntaxquote/bootstrap/cache/lifecycle acceptance.

Next isolated hash-map prerequisite now has6focused regressions (initial4 executed
BEFORE68971 terminal101:0pass/4fail; corpus/canonical-root additions not executed).
Fresh35primary compile23130+Node/compare terminal0 at actual pinned source. Native
failures remain explicit: map9Language error, publicdissoc/assoc! unresolved.
Retained complete publicdissoc/assoc!/dissoc! source with docstrings/allarity/branch
protocol dispatch using explicit defnbootstrap; exactextraction/EPL/patch hashes,
208importfiles and304partialreviews+761unassessed checkpass. These new source
helpers have not yet executed; do not claim they or HAMT acceptance pass.
ParentPR138 source remains frozen48c4b08, full75027 still running in otherworktree;
no overlapping native Cargo run. Next retain complete HAMT/TransientArrayMap/node/
sequence algorithms and public dependencies, sourcearraypop adapter and canonical
GC transport/guard coverage. Run focus after parentterminal, then freshprimary
andfullbaseline/review/final-headCI before opening readiness. Full M3 &env,
locations/syntaxquote/bootstrap/cache/lifecycle remain original uncompletedscope.

Hash-map helper focus92476 terminal101:0pass/6fail during core compilation because
second missing. Retained complete pinnedsecond/nnext and ITransientMap protocol
in dependency order, source/EPL/hashbounddefn adaptations;211importfiles and
307partialreviews+758unassessed verified. Helper focus75136 terminal0:2tests both
phases; added source ITransientMap probe verifies actualdissoc! fixed/variadic
protocol dispatch, receiveridentity and357ordered trace afterGC. Final62805
terminal0:3passed/0failed/0ignored, log
/private/tmp/suss-m3-map-mutation-helper-final.log. All threepublic mutation helper
algorithms now execute; complete hash-map acceptance still unfinished.
Combined10797 terminal101:0pass/6hashmapfail now Language instead of unresolved
names; Cargo stopped before helperbinary, so this command is not helper evidence.
Log /private/tmp/suss-m3-map-mutation-and-hash-focus.log. Remaining completeHAMT/
TransientArrayMap/nodes/sequences/sourcearraypop and canonicalGCtransport are next.
No blanketskips, fullworkspace success claim for this child or PR opened yet.
Parent806d701 dochead independently reviewed, production/tests identical to
48c4b08 full941/0/17/90 including47ABI. ExactCI36969751041 confirmedLIVE; parent
remainsdraft. No merges, issueclosures or M3 completion.

PR138 final806d701 CI36969751041 terminalSUCCESS. Downloaded complete log
/private/tmp/suss-pr138-final-ci.log:941passed/0failed/17existingignores/90groups,
all47ABI, Python88/import205/setup12 and final reader doctests. Independent
reviewer approved exact806 previously; remotehead rechecked unchanged. Marked
ready, no merge. This is parent evidence, not the new hash-map child's baseline.

Hash-map child retains complete HAMT/node/sequence/transient forms plus fix/quot,
array-copy-downward, not= and publicassoc, with exact original bytes/EPL and
explicit member/bootstrap/Error patches.246importfiles,342partialreviews and
723unassessed;17complete licensed setup statements verified. Python88passed.
Initial native source focus35733 missingquot;47126 failed generation and ran old
artifact (not newquot evidence);26785/77287 unsupported unadapted members;
35823 missingarray-copy-downward;49919 invalidWasm from own ceil import signature.
Debug13797 identified unaryceil incorrectly imported as binary; debug emission
removed and signature corrected.39758 terminal0:3helper regressions pass.
Factories initialized;51598 terminal101:0/6maps, canonicaltransport missing and
10Mdefaultfuel traps. Bounded100Mprobe96240 constructed33entries;97479 isolated
prototype publicassoc failure. Complete sourceassoc fixed it:23147 terminal0:1
persistent-root regression.23606 terminal101:4/6map tests pass, including actual
35primary observations in both phases, only canonicaltransport failures remain.
Bridge25314 terminal0:6/6map tests pass. Additional guard/numeric/popBEFORE17349
terminal101:7maptests+4helpers pass, transientarraymapremoval fails missingpop.
Ownedpop46266 terminal101:7maps+5helpers pass, copiedmethod.call fails existing
receiver-discarding closure adapter;85831 confirms that exact stage. Both native
call/apply now use existing anchored method dispatcher. Final focused13665 is
running, log /private/tmp/suss-m3-hamt-pop-call-focus.log, no final success claim.
New rawABI guard checks cover both push/pop callbacks and forged receiver buffers;
not yet executed. Tests use bounded100Mfuel explicitly; defaultbudgetperformance
remains a limitation. See docs/runtime/compiled-macro-hash-maps.md for remaining
NodeSeq/ArrayNodeSeqtransport/reify/undefined/formatting and fullM3 scope.
Next finish focused to terminal, fix failures, run required fullworkspace and
independent PR review, push significant fixes and require exact final-headCI.
No new PR yet, issueclosures, merge or M3completion.

13665 terminal101:7hashmap+6helper regressions pass, sourceapply3pass/2fail because
ordinary closures reached protocol-native-invoke and lost strict arity. Corrected
routing to only canonical Object native-wrapper tags; ordinary closures retain
originalinvoke. Focus25527 running same three binaries at final code, log
/private/tmp/suss-m3-hamt-pop-call-focus2.log. Earlier13665 is failed regression
evidence, not full sourceapply success. No skips or adjusted strict expectations.

25527 terminal0:7hashmap+6helper+5sourceapply tests pass (18total), log
/private/tmp/suss-m3-hamt-pop-call-focus2.log. Both phases pass35freshprimary
observations, nil/persistence/collision/transient/GC/canonicalguards and numerical
ceil/fix/quot boundaries. Native method.call/apply preserve physicalreceivers;
sourceapplystrictarityandorderedfailure regressions pass unchanged. Formatted
only affectedbridge/newtests/newpop/closurecall source. RawABI focus45049 isLIVE,
log /private/tmp/suss-m3-hamt-abi-guards.log; dependencyfeaturebuild, no rawABIpass
or fullworkspace claim yet. Python69963 terminal0:88tests,246imports,17setups,
342in-progressreviews+723unassessed. Next wait45049terminal before any othernative
Cargo run, then requiredfullworkspace; dispatch independentreview for draftPR
and fix significantfindings, with exactfinal-headCI required before readiness.

Published child draftPR140 at35efa9e15d481b721ec6e8b42c82dfaeb66da9fa,
https://github.com/bobby/suss/pull/140, basePR138806d701. Parent handoff-only merge
conflict resolved preserving both complete evidence records. RawABI45049
terminal0:1passed/0failed/46filtered, forgedcallback/emptyandforeignreceiver guards,
log /private/tmp/suss-m3-hamt-abi-guards.log. Requiredfullworkspace19934 confirmed
LIVE at35efa9e, log /private/tmp/suss-m3-hamt-full.log; early compiledphase/macro/
hashmap suites pass but full baseline is not yet proved. Independent reviewer140
approved exact35efa9e with no significant findings/fixes, independently verified
246imports/17setups/342reviews and inspected18focused+1ABI passing logs.
ExactCI36975001885 at35efa9e confirmedLIVE. Issue14 progress comment5946901899
records prerequisites and remainingoriginalacceptance. PR140 remainsdraft until
fullworkspace andfinal-headCI; no merge, issueclosure or M3completion. Nextpoll
same19934 untilterminal; resolve anyfailures and push fixes with newreview/finalCI.

Required fullworkspace19934 terminal0 at reviewed35efa9e; complete log
/private/tmp/suss-m3-hamt-full.log through final reader doctests. Verified954passed/0failed/17existingignores/92groups, including all47ABI.
Source remains unchanged; no new skips or suppressed failures. Exactfinal-head
CI remains required before readiness.

ParentPR140 full19934 terminal0 independently counted954/0/17/92, all47ABI/final
reader; dochead eec2aef independently approved, source/testsunchanged. ExactCI
36976094484 confirmedLIVE; old35run36975001885 cancelled/superseded, notfinalproof.
Parent remainsdraft until exactheadCI. Nextisolated sequenceworktree
/private/tmp/suss-m3-hamt-sequences branchportable/m3-hamt-sequences nowbasedeec2aef
with actualpinnedsubmodule, notsymlink. No parentimplementation changes.

Newsequence BEFORE23256 terminal101:0/2tests; bothphases all13cases executed,
11transportfailures (unknownNodeSeq/ArrayNodeSeq) +2collisionexecutionfailures.
Primary53448 terminal0 initial13observations; constructorliteralcollisionorder
reader-driven (key5first), variance against textualSussorder documented. Raw
initial corpus/observations saved /private/tmp/suss-m3-hamt-sequence-incidental-*
(not differential success). Orderedassoc primary98058 terminal0, all13exact,
key0..19collisionorder. Probe63095 terminal101 identifies persistentArrayMap ninth
assoc callsuninitializedinto; first2Symbolhash/key-test/transientops probe39102
terminal0. Initialportscript29006 failedTypeError beforeedits, thenregeneratedold
246artifact andranfirst2probe; notsourceinto executionevidence. Correctcomplete
sourceconj/transduce/into retained (allarity/docs/branches/metadata, onlydefnpatch),
249importfiles.98188 terminal0 after20persistentSymbolassocs, includingreal8->9
conversion. NewcanonicalNodeSeq/ArrayNodeSeqroots+boundedcursor/childtransport
71941 terminal0:3tests; depthguards protectrecursivecursors, sparseunusedslots
honorpinnednil?/undefined while rawundefinedsyntax stillrejected.

Extendedprimary73120 terminal0:27observations; existing13 unchanged,14newpublic
conj/into/transduce arity/metadata/nil/list/vector/xfcompletion/reducedobservations
captured frompinnedrun andstrictcomparepasses. Focus34576 terminal0:4newsequence+
7hashmap+6helpers+5sourceapply (22total), bothStores and27exactentry/numericresults.
Strengthenedmetadataassertions verify:doc7 exactly, oversized/cycliccursor and
validsparse/recovery guards; finalfocused78447 terminal0:4/4, log
/private/tmp/suss-m3-hamt-sequence-final-focus.log. All22combined results log
/private/tmp/suss-m3-hamt-sequence-focus2.log. Python62093 terminal0:88tests,
249imports/17setups/345in-progressreviews+720unassessed. No skips or fullcollection/
M3acceptanceclaims. See docs/runtime/compiled-macro-hamt-sequences.md.
NextpushdraftlinkedRefs14, dispatchindependentreviewer, requiredfullworkspace and
exactfinalheadCI before readiness. Then remainingiterator/reify/undefinedboundary
and originalrich&env/locations/syntaxquote/bootstrap/cache/lifecycle requirements.

PR141 published at08615ba, https://github.com/bobby/suss/pull/141, stacked on
PR140eec2aef. Independent reviewer approved exact08615ba without significant
findings/fixes; independently reran Python88/import249/setup17/review345 and
inspected focus/provenance/cursor semantics. Required fullworkspace3258 terminal0
at08615ba, log /private/tmp/suss-m3-hamt-sequence-full.log through final reader
doctests: 958 passed, 0 failed, 17 existing ignores across 93 result groups,
including all 47 runtime ABI tests. Source/tests remain unchanged.
CI36979640340 passed at08615ba; new documentation head needs
independent review confirmation and exactfinal-headCI before readiness. Issue14
comment5947608500 tracks originalcriteria/gaps, no issueclosure or merge.


Parent stack integration on 2026-10-02: GitHub confirms #133, #135, #136 and
#138 merged; #139 does not exist in this repository. Main35740aa has the exact
same full file tree as the former parent806d701 (9b2679e44830b0aed4b8b0c51cc0759a4efa4dfc).
PR140 repair merge c5f85eb retains the entire approved eec2aef tree
15ba77434785f008f41b40d316e0bd363a800c5a. Independent reviewer confirmed this
repair. The 954-pass local baseline remains applicable; its old CI run was
cancelled after approximately 60 minutes, and exact repair-head CI is required.
PR141 ancestry merge87caf9e likewise retains the entire reviewed e0fd886 tree
17c7f080b668d878ddd39e87d48846cb051e6b4e. Source-head CI36979640340 completed
successfully: independently counted958/0/17 across93 groups, including47 ABI
passes and final reader doctests. This handoff update requires final-head review
and CI. Neither PR is merged or ready without those checks. Original M3 criteria
remain open; next unblocked work is omitted-argument undefined storage semantics,
transient lifecycle enforcement and the rich compiled macro environment.


Transient hash-map lookup continuation (partial #14): isolated worktree
/private/tmp/suss-m3-hamt-lookup, branch portable/m3-hamt-lookup, parent PR141
8adf32d. Initial test probes exposed test API mistakes (49635 and before4.log compile failures
are not compatibility failures) and unavailable public hash-map (20930).
The permanent cases construct actual retained PersistentHashMap and use assoc;
no constructor fixture or substitute algorithm. First source execution30949
failed a test's number-layout assumption; corrected probe16440 terminal0 proves
omitted not-found already preserves ABI2 undefined. No new compiler adapter added.
Initial primary70681 failed missing generated base corpus; after oracle_cases.py,
fresh pinned run69487 terminal0 produced12 active observations, captured exactly
and strict comparison passes. The corpus checks nil?/identical? distinctions,
empty and nil-key branches, explicit defaults, IFn and persistent lookup.

Lifecycle BEFORE82160 terminal101: active undefined test passed, lookup after
persistent! failed the accepted-design assertion. Both complete retained lookup
methods now wrap existing bodies in the edit-token check; all active branches,
omitted argument and other methods remain. This is an explicit variance from
pinned TransientHashMap lookup, aligned with design section4. AFTER5293 terminal0
passed2 initial regressions. Strengthened focused15480 terminal0 passed25/0/0
(3 lookup,4 sequence,7 map,6 helpers,5 source apply), including both Stores,
forcedGC/captured closure, all key/default branches, argument effect order and
persistent-map recovery. Python68267 terminal0:88 tests,249 imports,345 partial
reviews/720 unassessed; fresh primary12 strict comparisons pass. Commands use
CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target CARGO_BUILD_JOBS=2
cargo test -p suss-cli --locked with the five named suites -- --test-threads=2.
Required full workspace, independent review and exactfinal-headCI remain pending.
See docs/runtime/compiled-macro-hamt-lookup.md. No issue or M3 closure. Remaining
iterator/reify, rich &env/locations/syntaxquote/bootstrap/cache and real session
scheduler/cancellation requirements remain the next unblocked work.


PR #142 independent review found the documented oracle runner selected the
nonexistent Cargo suite `compiled_macro_hamt_lookups`. The reviewer corrected it
to `compiled_macro_hamt_lookup`, verified the real target through locked Cargo
metadata and checked shell syntax without starting a concurrent native build.
Independent Python verification passed88 tests; generated import check verified249
files; review overlay verified345 partial/720 unassessed; the12 pinned lookup
observations matched exactly. Source/semantic tests were unchanged. Full workspace
and exact final-head CI remain required; no M3 or issue acceptance claim.


PR142 full workspace24875 completed terminal0 through final reader doctests.
Independent count of /private/tmp/suss-m3-hamt-lookup-full.log:961 passed,
0 failed,17 existing ignores across94 result groups, including all47 runtime ABI
tests. Exact reviewed head7d70b9f differs from7d16c08 only in oracle script suite
spelling and reviewer handoff; source and semantic tests are unchanged. PR body
records full evidence; final-head CI37013353157 remains live and required.
PR140 repair CI37012087148 and PR141 repair CI37012180540 also remain live.
No readiness or merge is claimed while exact-head CI is incomplete.

Compiled macro environment work is in /private/tmp/suss-m3-macro-environment,
branch portable/m3-macro-environment based on PR142 reviewed7d70b9f. This branch
is not yet a published PR or complete &env implementation. Analyzer lexical
lookup now retains actual LocalBinding records rather than only(id,type): reader
declaration/span/metadata, current lowered ID/type, let/loop/parameter/rest/self/
catch role, analyzed initializer and shared lexical shadow chain. Parameter
loop remapping updates ID without inventing another source shadow declaration.
Source analysis and saved lexical scopes share these records. ExpansionContext
exposes them to the host; CompiledMacros still does not yet marshal &env.
No initializer is re-evaluated to produce binding data. New Rust code is original;
no upstream source import/provenance count changed.

Initial compiler focus31419 terminal101 identified private remap_local visibility;
corrected focus32923 terminal0:2 native tests in both Stores verify real source
metadata/spans, initializer AST, lexical shadow restoration, catch roles, named
and variadic function arguments and actual results/effects once. Broadened89521
terminal0:9 native tests (2 records,3 transient lookup,4 source macro) and22 compiler
tests (14 closures,2 exceptions,1 if-let,5 nominal). Logs:
/private/tmp/suss-m3-macro-environment-{focus2,macro-focus,compiler-focus}.log.
Commands use CARGO_TARGET_DIR=/Users/bobby/code/github/bobby/suss/target
CARGO_BUILD_JOBS=2 cargo test -p suss-cli --locked with the three named suites,
then cargo test -p suss-compile --locked with the four named suites;
all include -- --test-threads=2. Full new-branch baseline/review/CI are pending.

Next concrete work: retain namespace definition/alias metadata, source file and
line/column context, function scopes and field declaration facts; marshal the
actual rich environment as canonical compiled data and pass &env beside &form.
Validate context/locals/metadata against fresh pinned macro observations and
execute macros that inspect them. Do not substitute a scalar map of local names
or claim these records alone satisfy &env. Syntax quote/unquote/splicing,
deterministic gensyms, bounded reproducible versioned Java-free bootstrap,
complete cache identity/invalidation, evaluator removal and scheduler/lifecycle
acceptance remain open. Re-read live issues12–15 this turn: their original
acceptance criteria remain unchanged and no issue/milestone closure is claimed.


Macro environment source/namespace facts checkpoint (not complete &env): actual
immutable source text and canonical file paths now accompany analysis, lexical
records and definition declarations. SourceOrigin maps UTF-8 offsets to one-based
UTF-16 columns, handling LF, CRLF and standalone CR; invalid offsets and APIs
without original source remain explicitly unknown. Namespace snapshots retain
actual aliases/refers and definition metadata, docstrings, once flag and analyzed
initializer; these are compiler facts, distinct from initialized runtime values.
Compile failure does not publish partial declarations. Runtime initializer failure
preserves old live values, while compiler facts truthfully describe the new AST.

Fresh pinned development-only CLJS observation35551 terminal0 yielded source
positions [[3,12],[4,30],[5,11],[6,13]], including a non-BMP prefix and mixed newline
forms. Strict environment_origin_oracle.py comparison passed all4 observations.
New native fact suites71568 terminal0:6passed, both Stores; initial45442 had one
fixture path assertion failure, corrected to the actual canonical /private/var
path. Broader94378 terminal0:32passed across binding records/source positions,
source macros/imports/reload/REPL and namespace session suites. Verification75362 terminal0 proves Python88/import249/review345; compiler32328
terminal0 proves36passed across closures/exceptions/if-let/modules/
nominal/phase modules. Logs /private/tmp/suss-m3-macro-facts-{focus2,broader,
python,compiler}.log. All native commands use the shared target, two build workers
and --test-threads=2; no full new-branch baseline or independent PR review yet.

CompiledMacros still does not marshal &env. Next establish metadata declaration
positions, genuine function/context and field records against pinned observations,
then transport rich canonical environment data and execute inspecting macros.
Do not equate the Rust host inspection fixture with source-level &env support.
All original issues12–15 and their remaining acceptance criteria stay open.

PR140 independent review found that the CI job's 60-minute budget, based on an
older 34m38s baseline, now cancels the expanded required workspace command before
completion. Exact c5f85eb run37012087148 was canceled at60m18s; its complete
downloaded log /private/tmp/suss-ci-37012087148-cancelled.log contains62 completed
groups with747passed/0failed/12ignored, followed by cancellation during conformance.
This is incomplete CI evidence, not a passing baseline. Independent GitHub job
inspection also confirmed PR141 run37012180540 canceled at60m21s and PR142
run37013353157 at60m19s; their completed test groups had no failures. All three
inventory verification steps succeeded. The reviewer raises only the bounded job
allowance to90minutes; the complete cargo test --workspace --locked --
--test-threads=2 command, CARGO_BUILD_JOBS=2 and every verifier remain unchanged.
No native source/tests or skips change. Prior local full954/0/17 proof remains
applicable, but exact new-head CI must complete successfully before PR readiness.
Next propagate the reviewed workflow repair to children and inspect complete
final-head runs; further timeouts remain failures requiring investigation.


PR140 reviewer pushed CI budget fixf9087cb, independently reviewed and propagated
through PR141e929de9 and PR142531ff53; both child reviewers approved exact heads.
Only workflow timeout60→90/comment and16handoff lines changed; all verifiers/full
workspace/two-worker limits remain. Exact CI37022116674/37023033025/37024044072
are in progress, all PRs draft. Issue14 comment5955330508 records incomplete old
canceled runs and remaining acceptance scope. No merge or readiness claim.

Macro source position continuation: pinned metadata-local probe79057 terminal0
observed [[3,18],[4,27],[5,38],[6,48]], locating actual variable tokens after plain,
tagged/chained/map metadata. Initial33663 failed only a misplaced development
fixture path; corrected primary43377 terminal0 uses the retained fixture/macro.
SourceOrigin.symbol_position validates actual reader syntax and symbol spelling;
unknown/generated mismatches remain unknown. Reader spans/metadata are preserved.
Initial native54220 terminal0:3positions passed; after final assertions/restoring
incidental formatter output,77206 terminal0:7facts passed (4binding+3position),
log /private/tmp/suss-m3-metadata-positions-final-focus.log. End-to-end oracle76191
terminal0 rebuilt both pinned development namespaces, compared all8 exact position
facts and executed3native tests, log /private/tmp/suss-m3-environment-origin-end-to-end.log.
All source tests execute real artifacts and independently decode after GC in both
Stores. No source-level rich &env support is claimed. Only this turn's80 proven
formatter-only changes were removed; all intended source work remains.
See docs/runtime/compiled-macro-environment-facts.md. Next full new-branch workspace
baseline, then genuine function/context/field facts and rich compiled transport.
Original issues12–15, bootstrap/cache/evaluator removal and scheduler acceptance
remain open; this local branch is not a published PR.


Isolated next worktree /private/tmp/suss-m3-macro-context, branch
portable/m3-macro-context at parent54e9789, actual pinned submodule initialized40062
terminal0. Parent baseline38391 remains live on unchanged54e9789, log
/private/tmp/suss-m3-macro-environment-full.log; no concurrent native build started.
New uncommitted field-fact implementation replaces private(field-Hir,mutable)
tuples with FieldBinding declarations/origins/index/mutability/actual access HIR,
exposes fields to ExpansionContext and retains genuine field shadows on locals.
Method parameters continue to take lexical precedence while retaining the field
they shadow; field reads/setters/caching-hash still use the same lowered accesses.
A new native field regression is written but NOT executed yet; compile/runtime
validity remains unproven until the parent baseline releases the shared target.

Fresh pinned field environment compiler96505 terminal0 and Node execution returned42;
full raw analyzer observations /private/tmp/suss-m3-field-environment-primary.edn
show field declaration/mutable metadata and a nested parameter's field shadow.
This is upstream evidence, not native differential success. Development-only
fixture/macro files are in ignored tests/oracle/out/generated/suss_oracle.
Next poll parent38391 to terminal and inspect its complete log; then compile and
execute new field-record/nominal/scope regressions, fixing actual failures. Continue
with genuine function scopes, three-way analysis context and rich environment
transport. Existing statement/tail booleans are not blindly mapped to pinned
:context. Original M3 scope stays open; no new PR is published or merged.


Field/context continuation now has actual native evidence. Focus86971 terminal0:
9passed (1context,4binding,1field,3sourcepositions), both Stores; log
/private/tmp/suss-m3-macro-context-focus.log. Broader23398 terminal0:24passed across
context/field/source macros/caching-hash/exceptions/native objects/type method
scope suites, log /private/tmp/suss-m3-macro-context-broader.log. Fields retain
actual lowered accesses and shadow metadata without executing reads. Analysis
contexts explicitly distinguish Statement/Expression/Return, with recur tail
checks separate. Context test independently decodes9result strings after GC and
compares16fresh pinned context facts by label; it does not claim expansion-order
agreement. Pin32985/34910 observed finally/catch/body visit order while Suss still
uses body/catch/finally analysis. This observable compile-phase ordering gap needs
an ordered regression and repair; runtime try/finally effects remain source-correct.

Oracle continuation16389 reused cached cljs output and produced no newly located
trace; strict checker rejected missing trace rather than accept stale success.
Force-compilation77212 terminal0 (:force true/:cache-analysis false) created a
fresh16-call trace; Node and strict context_facts_oracle.py agree with all16calls
and9results. scripts/test-context-facts-oracle.sh uses those options for real macro
execution each run. No source-level &env or M3 gate is claimed. PR143 reviewer has
pushed significant nested-definition record fix55e4f64; parent full baseline must
be rerun for that source and child must integrate it. Next ordered phase-expansion
regression/repair, bare try context check, function scopes and rich environment
transport. No new child PR or full child baseline yet.

Macro-facts full workspace38391 terminal0 at54e9789 completed through final reader
doctests. Independently counted /private/tmp/suss-m3-macro-environment-full.log:
968passed/0failed/17existingignores across96 result groups, including all47ABI.
No source or tests changed during that baseline. Full rich &env, bootstrap/cache/
evaluator removal and all remaining original M3 acceptance remain open.
Next publish this bounded prerequisite as draft Refs14, dispatch independent review,
push significant reviewer fixes and require exactfinal-headCI. Continue field/
context/function facts and actual environment transport in isolated child work.

PR143 independent review found a significant compiler-fact consistency defect:
an inner definition of the same global replaced declaration metadata, then the
outer analysis updated only its initializer, mixing two source declarations.
The reviewer now retains each definition's own record and publishes it together
with its initializer when analysis completes. In-progress inner declarations
remain observable while their bodies are analyzed; runtime evaluation is unchanged.
The new executing regression checks inner defonce/private/no-doc facts and final
outer def/doc/Do-initializer facts in both Stores, with the live value preserved.
An initial test fixture incorrectly gave defonce a docstring; after correcting
that unsupported arity, native session25609 failed at the actual outer.once
assertion (/private/tmp/suss-pr143-nested-before-valid.log). After the fix,
session47195 completed 5 binding-record tests, 0 failures, 0 ignores
(/private/tmp/suss-pr143-nested-after.log), using the shared target and two workers.
Oracle Python parses, shell syntax and git diff whitespace checks pass.
The prior 968/0/17 full baseline predates this source fix; rerun the full baseline
and require exact final-head CI before readiness. Rich compiled source &env and
the original M3 acceptance remain open. No PR was merged.


PR143 significant reviewer fix55e4f64 independently approved: nested same-global
DefinitionInfo now stays paired with its own initializer. Parent ff-integrated it;
required new-source workspace6974 is LIVE at55e4f64, log
/private/tmp/suss-m3-macro-environment-reviewed-full.log. CI37029058491 is LIVE on
exact55e4f64; prior849 CI canceled/superseded, not final evidence. Child merge
 e439354 preserves all field/context code and both handoff records. No merge of a
GitHub PR or readiness is claimed. Root will not start another native build until
6974 terminal. The initial full968 result does not cover the reviewer source fix.

Ordered-context regression continuation remains uncommitted. Fresh force-compiled
primary35792 terminal0 + Node yielded17compile-time context calls and10executed
result strings. It confirms bare try with no handlers is :expr and the pin visits
finally/catch/body during expansion. Golden was captured from this actual trace;
strict checker17/10 passes. Native test now retains ordered calls and includes
bare try, but has NOT been executed; run it BEFORE production repair when6974
releases the target, record genuine failure, then fix analyzer traversal/context
and rerun focused+exception/runtime effect tests. Preserve runtime operand/body/
cleanup ordering while matching actual compiler-phase macro effects. Rich source
&env, genuine function scopes and original M3 acceptance remain unfinished.


Exact final-head CI now succeeds for PR140f9087cb/run37022116674,
PR141e929de9/run37023033025 and PR142531ff53/run37024044072. Complete downloaded
logs /private/tmp/suss-ci-{37022116674,37023033025,37024044072}-success.log
independently count954/0/17/92groups,958/0/17/93groups and961/0/17/94groups;
all include47ABI and final reader doctests. ANSI escapes were stripped for the
reader heading check. All three exact heads have independent approval; PR bodies
updated and draft flags removed, OPEN/mergedAt:null/autoMergeRequest:null confirmed.
No GitHub PR was merged and no issue/milestone closure is claimed.

PR143 new-source local full6974 terminal101:63completed groups358/0/0, then Cargo
could not execute expected suss_compile-af924c6051b608e3 because it was absent.
Compiler unit tests and remaining baseline never ran. This is incomplete evidence,
not a passing full baseline or a source assertion failure. Cause is not established;
process inspection later found only root's next native Cargo/rustc. No unrelated
files deleted. Required full55e4f64 rerun still required; exactCI37029058491 live.

Ordered context BEFORE37228 terminal101 confirmed bare try :return mismatch;
strengthened BEFORE68215 terminal101 records actual body/catch/finally expansion
order versus pinned finally/catch/body, plus bare context mismatch. Logs
/private/tmp/suss-m3-macro-context-order-before{,2}.log. Production now preserves
bare-try caller context, uses return context only with handlers/cleanup, and
analyzes finally/catches/body while emitting unchanged runtime body/handler/cleanup
regions. AFTER50907 terminal0:17passed (1context,5binding,1field,10exception),
including both Stores, exact ordered17primary contexts and10decodedresultstrings,
GC and non-tail recur rejection. Log /private/tmp/suss-m3-macro-context-order-after.log.
Next finish end-to-end oracle runner, rerun full PR143 baseline in an isolated
owned target to avoid the unresolved shared-artifact disappearance, then final
child compiler/full checks and genuine function scopes/rich source &env transport.
Function scope primary42853 terminal0 and Node produced actual name stacks:
[["hinted"],["n"],[],["outer","inner"],["outer"],["declared"]]; raw records in
/private/tmp/suss-m3-function-scopes-primary.edn. This is upstream-only evidence;
no native scope records or compiled rich &env are implemented yet.
PR143 reviewer followup found a significant oracle freshness defect: both position
cljs.main invocations could reuse cached compiled output after macro-source edits.
The root's context oracle observed this reuse, then obtained a fresh trace with
:force true and :cache-analysis false. The reviewer independently checked pinned
compiler.cljc's force recompilation branch and analyzer cache option handling,
then applied both options to each position invocation. Node, strict eight-fact
comparison and the complete native position command remain unchanged. Shell syntax
and whitespace checks pass. This runner/docs-only fix does not alter Rust source;
the live isolated baseline at55e4f64 remains applicable to native source, but exact
new-head CI is still required. The reviewer did not rerun JVM/Node or native
position tests during this followup; prior position evidence is not relabeled as
fresh evidence for the repaired runner. No readiness or M3 completion is claimed.

Child context checkpoint8434499 incorporates reviewer143 runner repair7f821ac.
End-to-end context runner5425 terminal0 freshly checked17orderedcalls/10results
and the executing native context test; log /private/tmp/suss-m3-context-facts-end-to-end.log.
Parent isolated full95872 remains running at native source55e4f64, log
/private/tmp/suss-m3-pr143-isolated-full.log. Exact new-head143 CI37035852942
is running; neither readiness nor full success is claimed. Next run the repaired
position runner freshly, child compiler/full baseline, then mandatory child review.

PR144 independent review found significant missing return contexts at generated
let boundaries in as->, multi-operand and/or and case. Fresh pinned primary47661
and then complete retained-corpus primary69832 both terminated0 with force true
and analysis caching disabled; Node executed the generated artifacts. The expanded
corpus records24 ordered calls and15 independently returned strings. Before-fix
native83869 terminated101 at the ordered-call assertion: five positions incorrectly
reported expr instead of the pin's return; one-operand and correctly stays expr.
Log /private/tmp/suss-pr144-context-before.log. Reviewer now uses returning() only
for the generated let bodies; initializer contexts, tail legality and runtime
operand/branch ordering are unchanged. AFTER38274 terminated0:22 passed/0 failed/
0 ignored across context, field records, case, control flow, exceptions and type
method scopes, two workers/jobs2, isolated target /private/tmp/suss-m3-pr143-target.
Log /private/tmp/suss-pr144-context-after-valid.log. The initial after command used
an incorrect nonexistent test target and exited before executing tests; it is not
validation evidence. The field fixture now also checks a direct method parameter
colliding with an actual mutable field: declaration/shadow/access facts, lexical
precedence and actual42 result after GC in both Stores. Strict expanded checker
passes24/15 against freshly generated artifacts. Python AST and shell syntax,
whitespace checks pass (py_compile initially could not write an external macOS
cache path; AST parsing requires no cache write). Full final-source baseline and
exact-head CI remain required; rich source &env and original M3 work remain open.
Final corpus cardinality assertion was also executed in native61710 terminal0:
1 context test passed, log /private/tmp/suss-pr144-context-final.log.
