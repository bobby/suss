## Explicit Macro harness capability and char dependency (2026-11-23)

The harness now explicitly installs a separate Macro observer which always
raises on capture. It shares no Runtime decoder, snapshots, callbacks or token
stream. `run-test.py explicit-macro-hook-errors --test session_observation_hook`
passes4 tests, inputs unchanged, including failed capture propagating as an
expansion error with zero Runtime effects.

`run-suite.py full-suite-after-phase-local-observer` completes on the recorded
58328f5b patch, all recorded inputs unchanged. Build/lock return0; native suite
comparison returns101 after400.877s. Only EDN's existing diagnostic changes,
from absent Macro host service to unresolved Runtime char. All297 EDN assertions
remain unexecuted. The audit preserves4917 strict prior passes and4932 separately
comparable prior rows, unchanged counts/skip rules and all assertion/test outcomes.
No baseline is written. The audit permits exactly the typed UUID projection and
seven-line explicit Macro service setup; source bodies, routing, Runtime snapshot
capture and comparison remain byte-identical to the parent.

`results/m12-character-coercion/oracle.py` records94 fresh JVM1.12.6 controls:
direct/source-primitive char differs from first-class/apply/Object conversion for
fractional doubles and NaN. Full upstream char source, JAR/source hashes and
development RT bytecode are retained with EPL provenance. The earlier oracle
attempt has exit1 and66 partial rows because a surrogate character literal is
invalid JVM source; it is not a passing run. Surrogate controls instead construct
the unit numerically. `run-test.py before-char-coercion --test clojure_char`
terminates101 after61.272s, inputs unchanged, at unresolved char. No missing
result is counted as passing; the accepted one-unit String/Character platform
identification remains explicit in three controls.

## Analyzer import setup and phase-local observation services (2026-11-23)

The required unfiltered workspace baseline on3797c2b8 terminates101 after777.907s,
all recorded inputs unchanged. It completes49 tests and finds two analyzer round-
trip regressions before later targets execute: macro BigInteger source and the
user-form corpus gain an extra Runtime namespace node on each emission cycle.
The native suite comparator's separate3797c2b8 rerun passes, inputs/executable
unchanged, with a byte-identical full capture. Neither result is a complete
workspace pass; both original receipts/logs are retained.

Macro import setup reuses the original namespace declaration internally. The
analyzer now suppresses recording only during that setup, preserving the outer
recorder/pending HIR across nested setup and errors. Real namespace/macro nodes,
source origins and phase environments remain recorded. `run-test.py
 after-internal-import-recording --test analyzer_edn` passes all16 tests with
unchanged inputs, including both original failures and a new two-namespace
regression requiring exactly six source nodes, retained Macro aliases and a
complete emitted-source round trip.

CompiledMacros can explicitly install its own native observation service. It
shares the existing phase-scoped installation machinery but inherits no Runtime
callback, cell or value; the public Session hook keeps its Runtime-only guard.
The3 focused observation tests pass on the recorded patch, including failed
Macro imports without opt-in, recovery with a separate callback, and independently
observed Macro41/Runtime52 payloads returning17/900. Duplicate installation,
fixed arity, namespace override and Runtime reset guards remain covered. This
capability alone does not change the suite harness or establish EDN execution.
Current CI/full-workspace and EDN acceptance remain separate evidence on GitHub.

## Audited UUID diagnostic changes (2026-11-23)

`run-suite.py full-suite-after-inline-source-macros` completes on5ee039cf,
inputs unchanged: build/lock return0; native comparison returns101 after313.487s.
The full5002-row capture preserves all4917 strict prior passes and4932 separately
comparable prior rows. Counts remain4842 host matches,75 guest judgments,
86 failures,1936 unexecuted, zero skips and72 failed namespaces.

`audit-suite.py` finds only four namespace diagnostic changes: identity advances
from UUID to unsupported instants; parse-uuid/uuid? reach missing parse-uuid;
EDN reaches its Runtime-only harness observation service while preparing Macro
imports. Its297 assertions remain unexecuted. The isolated Macro session does
not inherit Runtime host capabilities. No host callback, fabricated observation,
vendored-body rewrite or expanded phase authority is introduced to bypass this.
Decimal operands remain194 pass/6 unexecuted; BigInt/Ratio214 pass/15 unexecuted.

`write-reviewed-reader-baseline.py` checks the source-frozen receipt, unchanged
primary SHA, exact preserved pass/count/skip results and all four diagnostics
before updating only those existing namespace entries. Assertion/test failures
and comparator rules stay unchanged. The separate host comparator returns0;
the original native101 remains retained, not relabeled a native pass. Compressed
raw receipts/logs/capture retain their byte hashes in archives.json. Current
workspace and final-head CI results must be reported separately. The EDN phase
host-service dependency remains an acceptance limitation on GitHub.

## Compiled namespace-local declarations and rollback (2026-11-23)

`run-inline-gates.py inline-source-origin-and-cache-work` passes67 tests with
all recorded source/fixture inputs unchanged:40 namespace/frontend/source-
metadata tests, one actual linker-failure rollback test,13 UUID tests and13
original record/namespace/artifact guards. Runtime and Macro namespaces can
now compile local defmacro declarations through the isolated compiled host,
including imported functions/macros, sequential declarations and source data
retained after GC. The shared script/AOT declaration path retains each namespace's
imports and original file positions. Actual AOT checks line5/column15/the exact
source filename and once-only Runtime initialization. Failed reload, later script
compilation failure and real missing-linker-import failure preserve previous
bindings/exports and recover. Successfully initialized Macro dependencies remain
loaded without replay. Source contexts distinguish original inputs and top-level
heads; dependency identity hashing is charged before allocation/hashing.

All15 foundation checks pass, including364 Python tests, in
`results/m12-case-budget/inline-source-foundation`, inputs unchanged. The earlier
67-test run and original failing regression remain retained. The low-level raw-
core fixture reaches the UUID shipped-core guard after declarations prepare;
its source and failure are preserved separately. The actual module regression
uses the ordinary shipped-bootstrap constructors in both phases.

Nineteen additional fresh JVM UUID case controls use independently read selectors
and nested collection constants. All17 value/effect controls match through Runtime,
Macro and actual CLI AOT; both duplicate-constant controls reject through actual
source compilation in all three routes. The private value-hash branch already
exists; these controls establish its behavior without relying on pooled literals.
No primary value, count/skip rule, failure baseline, original budget or size
threshold changes. Full EDN/full-suite execution, the unfiltered workspace run,
final-head CI and independent promotion review remain required evidence.

CI45196497382 on24da88f4 has completed failing jobs1/3 for the published pre-fix
inline-defmacro regression and known-failure diagnostic drift. Both byte-exact
job logs are retained before cancelling the remaining redundant work. Cancellation
is not a passing result; PR505's promotion workflow is untouched.

## Source namespace macro dependency and fresh core probes (2026-11-23)

`run-suite.py full-suite-after-clojure-edn-namespace` finishes on c42b75bf,
recorded inputs unchanged: build and lock checks return0; native suite returns101
after754.263s. The reader namespace now loads, exposing `Unresolved Runtime name
defmacro` at bytes441..449 in the unchanged vendored EDN namespace. Its local
`are-read-as`/`are-thrown` declarations therefore prevent all297 EDN assertions
from executing. The audit again preserves4917 strict prior passes and4932
individually comparable prior rows; primary values/count/skip rules and the
reviewed baseline remain unchanged. Counts are4842 host matches,75 guest
judgments,86 failures,1936 unexecuted, zero skips and72 failed namespaces.
Decimal operand observations remain194 pass/6 unexecuted; BigInt/Ratio operands
remain214 pass/15 unexecuted. This is a failing full-suite comparison.

The independent `inline_module_macros` source-module regression fails before
the repair with the same unresolved Runtime `defmacro`, at bytes54..62 in its
own source namespace. `run-test.py before-inline-source-module-macros --test
inline_module_macros source_namespace_local_macros_use_phase_imports_and_expand_following_forms`
records terminal101 after32.099s with inputs unchanged. Runtime loading fails;
the later Macro iteration is not executed. The test requires real compiled
transformers, phase imports and independent Decimal/UUID storage after GC.
Namespace loading needs the same declaration/staging discipline as the compiled
script path; no test-only macro interpretation or vendored-body rewrite is used.

`results/m12-core-dependencies/oracle.py` records396 fresh JVM Clojure1.12.6
controls for min-key and some-fn. Full upstream forms, EPL notice, source/JAR
hashes and def/fn adaptation patches are retained. Key functions execute once;
min-key uses < for the first pair and <= thereafter, preserving ties/NaN. The
some-fn controls cover1..5 predicates,0..6 arguments, every hit position,
short-circuiting, returned truthy values and exact predicate/argument traces.
`run-prototype-repl.py` executes396/396 matching rows through the actual CLI,
with each request at an explicit ordinary10M compilation allowance, executable
hash unchanged. These are local definitions, not a production-core, Macro or
AOT pass. Six pure map-indexed fixture constructors are materialized because
that helper is unported; separate fresh JVM execution verifies all396 values
identical to the untouched primary controls. No function under test changes.
Earlier prototype failures (missing println, batch analysis fuel, unavailable
map-indexed and an unsupported -e compilation option) remain terminal receipts
with zero observed controls, never passes. No production budget is changed.

## UUID full-suite capture and EDN namespace dependency (2026-11-23)

`python3 results/m12-reader-followup/run-suite.py full-suite-after-uuid-and-offset-repair`
completes on d64d424f with all recorded inputs unchanged. Build and suite-lock
return0; the native full suite returns101 after580.924s because four namespace
diagnostics advance beyond unsupported UUID. The complete5002-row capture and
logs are retained; this is a failing baseline comparison, not a passing full suite.

`audit-suite.py full-suite-after-uuid-and-offset-repair` independently compares
that capture with the unchanged pinned primary, count/skip rules and a783ad60
parent. It preserves all4917 strict prior passes and separately all4932
individually comparable prior rows. Counts remain4842 host matches,75 guest
judgments,86 failures,1936 unexecuted, zero skips and72 failed namespaces.
The297 EDN assertions remain unexecuted: after UUID constants read successfully,
namespace loading reaches the missing `clojure.edn` source. Identity similarly
reaches unsupported `#inst`; parse-uuid and uuid? suite namespaces reach missing
`parse-uuid`. No baseline is written by this audit.

The auditor's parent-capture control reproduces the original outcomes. Its
negative control removes one actual Decimal-operand observation and requires
rejection, preserving the distinction between missing observations and passes.
These two controls are synthetic audit tests, not Suss execution evidence.
Actual Decimal operands remain194/200 passing with6 unexecuted; actual BigInt/
Ratio operands remain214/229 passing with15 unexecuted. Missing min-key, some-fn,
real eval and the EDN namespace remain explicit dependencies. A closed issue
label alone does not establish its suite acceptance criteria.

A shipped `clojure.edn` compatibility namespace now requires `suss.edn` and
exports its exact compiled `read-string` function in both phases. It introduces
no test-harness rewrite. `run-test.py clojure-edn-compatibility --test clojure_uuid
clojure_edn_namespace_exposes_the_same_data_reader_in_both_phases` returns0:
one test exercises Runtime and Macro, function identity, UUID/Decimal values,
EOF options, configured/default reader precedence and rejection of reader eval.
The311.143s command records unchanged inputs. This focused result does not
certify execution or passing of the complete EDN suite body; a fresh full-suite
capture follows. Earlier failures remain in their original captures.

## UUID source loading and compiler membership observations (2026-11-23)

`run-source.py typed-offsets-and-source-safe-uuid-table` passes144 focused tests
with all recorded inputs unchanged:30 UUID/record/source-core integration tests,
60 graph/storage/reader-budget tests, the unchanged artifact-size gate, and53
independent decoder/privacy/source-tooling tests. Membership work is361130 and
368956, below the original one-million limit; the four-million record limit also
passes. Artifact growth remains2352 bytes/form and96 extra split-form bytes,
within the unchanged3900/128 limits.

Compiler-generated membership offsets now use a typed long-vector recipe. This
preserves the exact vector/schema and UTF-16 boundaries while avoiding separate
reader-key planning for each structural offset. Wide signed values survive GC
in both phases. Final native key/storage tracing retains the earlier1026-entry
growth boundary. The first attempted index-sharing cache increased costs to
1020485/1167745 and was removed; its exact source patch and failed receipts remain
under `interned-membership-indices`.

Ordinary source-core loading previously threw `Mutable fields are private to
their type` while constructing the UUID parser's37-element vector. A private
primitive array now stores that immutable parser table. Source-core loading and
UUID reading pass in both phases without changing field-access policy. Existing
hash, reduction, vector, source-macro metadata and reader-binding regressions pass.

`run-roundtrip.py uuid-source-roundtrip-and-ci-regressions` passes27 additional
tests. The UUID AST records real `:const`/`:uuid` values; native EDN and emitted
source round-trip losslessly, and the emitted source executes in both phases.
The remaining visitor/projection arms preserve typed UUID bits. The retained
when-let provenance hash is aligned with UUID binding-key hashing; the checker
and its captured upstream/native evidence remain intact.

The original CI45175714660 failures and all exploratory failures remain retained.
Full-suite/affected EDN capture, complete workspace execution, final-head CI and
independent promotion review still need their own results. The prior foundation
run passes15 checks, including364 Python tests, on its recorded snapshot; further
source changes require their final check. No complete BigDecimal or milestone
acceptance follows from these focused gates.

## Compiled UUID integration controls (2026-11-23)

`run-source.py after-uuid-duplicate-format-and-macro-registration` passes all85
reader-crate tests and all8 compiled integration tests, with every recorded input
unchanged. The172 original UUID expression controls,235 runtime parser/error
controls and12 signed-boundary/collection/quotation controls execute against fresh
JVM Clojure1.12.6 results in both Runtime and Macro. Actual CLI AOT compiles and
executes the184 expression/boundary controls. Canonical128-bit fields survive GC
in both phases with public equality/hash/printing disabled; malformed/foreign
storage remains rejected by the independent decoder. Field/prototype sealing and
private helper guards pass too.

The source reader retains a typed UUID scalar; quotes, source emission, native
EDN, macro data, raw source keys and analyzer graphs preserve signed halves.
Original core code selects explicit64-bit operations without changing the32-bit
operations used by existing ClojureScript ports. The full positive signed-long
parser range is retained. UUID duplicate diagnostics use canonical `str`, matching
the JVM. Whole upstream class forms, adaptation patch, predicate source, EPL
notice and exact block hashes are retained by `provenance.py --check` and
`docs/compatibility/bootstrap/clojure-uuid.json`.

Earlier compilation, fixture,32-bit truncation, parser-boundary and duplicate
message failures remain in their separate terminal receipts and logs. None is
counted as a pass. New focused integration evidence does not establish the
complete affected EDN body, unchanged full-suite comparison, record/AOT budgets,
full workspace baseline, final-head CI or independent promotion review. Those
checks still need execution. Regex, instants and real runtime eval remain separate
numeric-suite dependencies; complete BigDecimal acceptance is unchanged.

# UUID and regex reader dependency observations

## Native tooling EDN UUID observations (2026-11-23)

`run-focused.py native-edn-uuid-after-source-projection` passes all four focused
UUID tests. Native EDN now retains typed128-bit values, canonical printing/key
duplicate checks, configured-reader precedence, builtin-before-default behavior
and lossless source-data projection. Serde rejects unsupported UUID conversion
rather than silently turning it into a string. Original missing match-arm
compilation failure is retained in `native-edn-uuid-gates`.

`run-reader.py native-edn-all-reader-gates` passes all84 reader-crate tests with
zero failures/ignored tests and recorded inputs unchanged. This is native tooling
EDN evidence only. Source reader selection, compiled Runtime/Macro UUID values,
public predicates/functions, macro transport, retained-value/GC and actual AOT
still require integration. Earlier native-parser-only results retain their own
source snapshots; none establishes complete UUID or BigDecimal acceptance.

`python3 results/m12-reader-followup/oracle.py` records204 fresh pinned JVM
Clojure1.12.6 expression controls. UUID inputs include short, upper-case,
plus-prefixed, over-width and Unicode hexadecimal groups; regex observations
retain raw pattern text and identity semantics. These are reference results,
not claims that Suss implements either runtime value yet.

`run-before.py` executes a copied pre-repair source2254f70c CLI with binary
SHA256969bbcd27acb249c24c2d52ed43be6727c7f1e3685a508883fca39fdd3effff3.
All13 selected native/AOT commands fail explicitly: unknown uuid tag or unsupported
regex dispatch. Input text and the exact commands/output are retained. They are
reader/compilation failures, not executions of macro or UUID runtime values.

`oracle-uuid-parser.py` independently records235 UTF-16 UUID input cases from
actual `java.util.UUID/fromString`:179 accepted and56 rejected. Complete signed
halves, canonical text, hashCode and error-message UTF-16 units are retained.
The same JVM records Character.digit(char,16) for all65536 units. Source/JAR
hashes, OpenJDK bytecode and commands qualify these primary observations. The
initial giant inlined fixture exceeded the JVM method-size limit; its source,
terminal failure and command remain under `uuid-parser-oracle-too-large`.
The subsequent recording reads the same data as EDN rather than embedding it
in one bytecode method; no primary cases were dropped.

`run-focused.py uuid-parser-gates` passes all three focused Rust tests with
unchanged inputs. The original parser retains immutable128-bit data and lossless
UTF-16 errors, masks accepted groups only after checked signed-long parsing,
compares signed halves, and exactly matches the full BMP hexadecimal scan.
It has no Java dependency. This establishes a native parser/data helper only:
compiler forms, native EDN, Runtime/Macro values, readers, public functions,
macro transport, retained-value/GC checks, AOT and affected suite execution still
need integration. No issue acceptance, workspace or CI pass follows from these
three focused tests. Original BigDecimal and suite baselines remain unchanged.
## Complete char and EDN captures (2026-11-23)

`run-suite.py full-suite-after-reserved-core-head-transport` completes onbc2694b9,
recorded inputs unchanged:build/lock return0; native comparison returns101 after
544.732s. `audit-suite.py`, `audit-edn-body.py` and the character auditor pass.
The5308 raw observations preserve4917 strict prior passes and4932 separately
comparable rows:5123 host matches,81 guest judgments,105 failures,
1630 unexecuted, zero skips and70 failed namespaces. All nine original char
assertions match. All297 EDN assertions execute in order:272 host matches,
six guest judgments and19 explicit reader failures. Decimal199/200 and
BigInt/Ratio219/229 match; remaining numeric observations are unexecuted.

Only the reserved cljs.core/suss.core assertion-head namespaces use the JVM
clojure.core transport spelling; every raw source-head is retained. Both actual
bindings and compiled syntax quote are covered by the two focused regressions,
including rejection of arbitrary user heads, wrong operators/verdicts/operands.
Runtime snapshots, primary values, comparator, vendored bodies and count/skip
rules are unchanged. The isolated Macro observer always raises on capture.

`write-reviewed-char-reader-baseline.py` removes only the two executed namespace
load failures and adds exactly the19 observed reader failures. The separate host
comparison passes; the retained native101 is never relabeled as a native rerun
or workspace pass. Larger raw files have deterministic gzip archives and byte
hashes in the capture's archives.json; decompress those files at their recorded
relative paths before rerunning the auditors from a fresh checkout.

## Independent-review repairs and CI inventory (2026-11-23)

`before-review-repairs` retains native counterexamples against immutable existing
production artifacts:cache eligibility fails after a successful partial-import
retry (exit101,85.012s), and runtime UUID syntax quote throws unsupported data
(exit1,94.595s). The captured production inputs match3890 unchanged frozen source
inputs; the two authored native test files and fresh primary fixture are explicit
differences. These are not Cargo-regression or workspace passes.

`after-review-repairs` retains the first failed build:the original generator
source-identity guard catches stale copied project fingerprints before tests run.
After targeted project cache cleanup, `after-review-repairs-clean-project-cache`
passes1+2+60+17 native executions with input hashes unchanged. Three targeted
cases repeat in the broader group, leaving77 distinct tests. The original
13-test unboxed-locals target also passes at5269cf4f after requiring the three
appended char helpers beside the original37. Existing scalar/no-numeric-box checks
are byte-identical and run over all40 exports. No performance bound is relaxed.

All15 foundation checks pass, including364 Python tests. Current independent
reviews clear the repairs; full repaired-head workspace and CI are separate.
