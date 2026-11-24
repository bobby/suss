# Compiled public eval and real Vars

Native Runtime and isolated Macro sessions compile form data through their own
persistent analyzer/emitter and live binding cells. Public eval supports recursive
reentry, actual functions and Vars (including nested/quoted constants), dynamic
namespace/math context, top-level do failure boundaries and evaluated macros.
Macro declarations compile their real body once in the owning Macro Store;
captured roots survive redefinition, GC and public owner replacement. Cumulative
allowances and cancellation cover compilation and isolated calls. Pure AOT
retains the explicit missing-compiler-capability error and its import contract.

At `f3ae547d`, the scoped Var metadata follow-up passes all4 focused commands
with unchanged source/HEAD:93 tests across complete AOT option, metadata,
indexed-data, sealed built-in, public eval27, identity and stream targets.
The new Runtime/Macro ordinary/:const constructor replacement control and the
unchanged original AOT constructor-redefinition case also pass as targeted checks.
Compiler-owned metadata uses original core bindings while subsequent user
initializers retain ordinary resolution. The earlier current-source AOT101 and
cancelled baseline/CI attempts remain preserved; the earlier complete native0
below does not validate this later compiler change. Fresh broader/capture,
exact workspace, independent review and final-head CI remain required.

At `c3946719`, the corrected-source frozen broader gate passes all7 commands:
26 public eval tests,117 compiler host regressions,193 tests across25 selected
numeric/module/artifact/budget/analyzer/symbol targets, optimized random effects,
Java/Node-free bootstrap reproduction, actual complete native suite execution
and strict host review. HEAD, inputs and Binaryen remain unchanged. The focused
Var/Symbol repair cohort separately passes all76 target tests at `1876f27d`.

The fresh complete native suite returns actual0 in316.79s against the unchanged
reviewed baseline. Its5564 observed rows preserve all previous counts, failures,
skips and original opaque judgments; no baseline is written. All229 BigInt/Ratio
and200 Decimal tagged-operand rows remain independent host matches. The original
complete eval/symbol/var? bodies retain34/61/34 rows with29/61/26 host matches and
original5/0/8 guest judgments. The105 unrelated accepted assertion failures and
63 namespace failures remain explicit; this is baseline conformance, not whole-
language or complete numeric source-form acceptance.

[Fresh strict capture review](eval-broader-after-indexed-seq-repair/complete-capture/current-complete-capture-review.json)
and [actual native0 receipt](eval-broader-after-indexed-seq-repair/complete-capture/suite-commands.json.gz)
retain the new evidence. Historical101 attempts below keep their original source
qualification and outcomes. The exact workspace baseline, independent promotion
review and final-head CI remain separate gates.

At `7fde2815`, all25 public target tests pass, including27 fresh symbol controls
in both phases, and all6 retained identifier tests execute the original40-row
implementation corpus and every fixed/spread invocation guard. The complete
original eval, symbol and var? bodies execute34/61/34 ordered rows. Their host/
guest results are29/5,61/0 and26/8, with precisely the original opaque judgments,
zero failures/unexecuted/skips and strict host matches for eval's numeric rows.

The frozen broader run at `cfdd013d` passes the public/unit group, all23 selected
numeric/module/artifact/budget targets, optimized random effects and Java/Node-free
bootstrap reproduction. Its318.58s native suite capture returns the original101
against the former baseline. Strict host review then proves all5564 raw rows,
all5346 prior comparable successes, exactly129 newly executed rows and unchanged
primary/count/skip/guest rules. The five symbol failures from the earlier capture
are fixed, not added to the baseline. The measured update removes only the three
completed namespace failures and advances ifn?'s remaining failure from var to
promise. The unchanged host comparator returns0 against that reviewed update.

[Capture review](eval-broader-after-symbol-and-var-contract/complete-capture/complete-capture-review.json),
[baseline write qualification](eval-broader-after-symbol-and-var-contract/complete-capture/reviewed-eval-baseline-write.json),
[original native101 receipt](eval-broader-after-symbol-and-var-contract/complete-capture/suite-commands.json.gz),
and [adversarial review controls](test_complete_capture_review.py) retain the exact
evidence. Nine guard controls reject altered operands even with a pass verdict,
missing/reordered rows, changed guest verdicts, erased unrelated failures,
changed primary bytes and an invented remaining blocker. The actual earlier
five-symbol-failure capture is also rejected.

All229 BigInt/Ratio and200 Decimal tagged-operand observations independently
match. These are operand-subset results, not a claim of all numeric source-form
or whole-language conformance. Regex-blocked eq/not-eq and num namespaces remain
explicit in the baseline; numeric issue completion requires a separate scope/
acceptance audit. The exact two-thread workspace baseline, independent promotion
review and final-head CI remain separate required gates. No merge is authorized.

Historical development receipts follow. Each belongs to its recorded source and
does not replace the current complete-capture or workspace requirements.


Primary contains24 actual Clojure1.12.6 observations, the complete eval form,
EPL notice, exact JAR/entry/source/stdout hashes and original expressions.
They cover form data, arithmetic, functions, real binding-cell Vars, namespace
selection, macros, recursion, laziness, dynamic math context and failure effects.
These recordings alone establish no native implementation or numeric acceptance.

clojure_eval.rs requires all24 values in both native phases and an additional
reentrant compiled-macro path. Expected values are host-parsed; independent GC
storage observations retain scalar categories/Decimal scale, with only unordered
map/set observations normalized on the host. Fixture namespaces/definitions use
the ordinary compiled script path; no eval or result is supplied locally.

run-before.py links that actual test source against exact immutable pre-eval
libraries from the frozen PR507 target. The recorder itself runs inside a bounded
scope, freezes test/fixture inputs and hashes the selected libraries before/after.
It writes no Cargo cache. A native missing-name failure is a counterexample,
never a Cargo or post-implementation pass. The full34-assertion original body,
source publication and remaining issue acceptance have separate gates.

At4196f081 the durable private-capability gate returns0, inputs unchanged.
The real import-free runtime capability/GC/identity test passes (249.851s
including build; native execution3.35s). Two borrowed-Store compiler tests pass
in both Runtime and Macro (539.219s including additional workspace-feature
builds; native execution44.13s). They invoke the real compiler while a Wasm
caller is live, share existing binding cells and compile actual nominal form
data directly into Ratio/BigInt results; retained results survive GC. Neither
test supplies an eval binding or a fabricated language result.

The original1630.162s compile101 is retained with its incomplete recorder and
posthoc unchanged-input check; it contains two test API mistakes (F64 bits and
an extra ref_eq reference), fixed before the successful gate. The new recorder
writes progress to its owned log so detached stdout cannot interrupt its final
receipt. Persistent compiler publication, user macros, real Vars, namespace/
dynamic context, public eval, the24 primary controls and original34-row body
remain required. These private tests establish no public eval acceptance.

Atadb66ae9 the private runtime capability and all three borrowed-Store compiler
tests return0 (162.507/238.981s including builds), inputs unchanged. One shared
authoritative compiler catalog/cell map now serves owned inputs and callbacks;
no metadata lock crosses compilation-instantiation or source execution. The
new test publishes a callback-created binding to later owned inputs, sees later
owned definitions in a callback, and retains a compiled function over GC and
live root redefinition in both phases. The namespace getter returns an owned
snapshot rather than borrowing through the metadata lock. Existing session,
module, observation and bootstrap behavior needs broader regression checks.
Public eval, macro authority and real Var acceptance remain unimplemented.

The broader state gate at0d7f5e9e returns0, inputs unchanged:36 session library
tests, nine inline-module macro tests, four isolated observation tests and four
original bootstrap execution tests pass. Java/Node-free double reproduction
and compiler identity verification also pass (bootstrap command277.446s).
The expected caught panic in the existing nested-budget unwind regression is
retained in its log; the test and all36 session cases pass. This establishes
existing owned-session behavior after shared metadata publication, not public
eval, macro-authority sharing, real Vars or complete numeric acceptance.

At6c864a34, the borrowed session view regression returns0 (295.633s including
build; native23.67s), inputs unchanged. Runtime/Macro callbacks reuse the
existing FormBridge and invocation/compiler methods, compile actual form data
to3/2, and retain the result across GC. Owned Session remains Send. Borrowed
views cannot reset the owner, do not refill its budget, and do not retire its
pending requests on drop; owner teardown cancels exactly once. The negative
entry verifies the existing cancellation policy: consume the request, exhaust
fuel, and reject a retry as OutOfFuel without replenishment.

Earlier compile101 receipts retain the concrete-Store scope signatures and
unchanged assertions, then the original311.053s runtime101 retains a mistaken
sticky-flag test expectation. The corrected regression checks the pre-existing
consumed-request/zero-fuel contract; production consume_host_work is unchanged.
All failures remain archived. Existing session/module/observation/bootstrap
checks must pass before publishing this interface change as a new checkpoint.
No public eval or borrowed user-macro authority is claimed.

The broader borrowed-interface gate at e403e68d returns 0 with unchanged
inputs: 37 session library tests, nine inline module macro tests, four
isolated observation tests and four original bootstrap execution tests pass.
The Java/Node-free double bootstrap and compiler identity checks also pass.
Command times are 21.942, 86.937, 4.945 and 85.704 seconds respectively.
Full receipts and logs are retained in
`results/m12-eval/after-borrowed-session-interface`. These 54 regressions
validate the borrowed interface while public eval, shared macro authority,
real Vars and complete primary/body acceptance remain required.

At f8f78da5, the borrowed analysis graph gate returns 0 with unchanged
inputs. Runtime/Macro callbacks use the ordinary expansion-context transport,
then invoke a compiled transformer and compile its real result to 3/2 across
GC, without refilling caller fuel or retiring owner requests. All 60 existing
analysis-graph regressions also pass (393.767 seconds including the build for
the callback test; 8.329 seconds for the graph target). Full receipts/logs are
under `results/m12-eval/borrowed-analysis-first-native`. Shared user-macro
authority and public eval acceptance remain required.

At 5e89daf1, borrowed Macro authority uses one authoritative shared catalog
for definitions, provenance, loading, caches and declaration transactions. The
explicit typed evaluation-capability fixture expands an existing user macro
in the active Macro Store, publishes a callback-defined macro to later owned
inputs, and retains its result across GC. The gate returns 0 with unchanged
inputs: all 13 macro-library tests (including that fixture) and nine original
inline-module tests pass. Command times are 358.885 seconds including build
for the focused fixture, 12.016 seconds for the macro library and 121.126
seconds for inline modules. Full receipts/logs are retained under
`results/m12-eval/borrowed-macro-through-evaluation-capability`.

The initial duplicate-import compile failure remains archived. The subsequent
research run also returned 0, but its observation-hook fixture violated that
hook's non-reentrant contract and is explicitly unsuitable as capability
validation. The replacement uses evaluate-form directly; observation hooks
retain their contract. Public eval, the Runtime service handle, real Vars and
complete primary/body acceptance remain required.

At 61e24bef, the native Macro service owns the existing phase Store through
an exclusive owned lease. Preparation reuses its loaded macros and catalog;
the lease returns before Runtime execution and on request unwind. The direct
Runtime callback test expands the original user macro once per input, returns
a callable function across GC after dropping the public owner, and preserves
the existing macro-side effect counter without replay. The interruption handle
remains available while the host is lent. No Session pointer or self-borrowing
mutex guard is captured.

`run-installation.py runtime-macro-service-after-owned-form-api
--macro-service-only` returns 0 with unchanged inputs. There are 68 distinct
passing regressions: 14 macro-library, nine inline-module, 37 session-library,
four observation and four original bootstrap execution tests. The focused
service test is also run separately (464.193 seconds including build); the
subsequent command times are 15.200, 117.565, 24.929, 4.516 and 115.161 seconds.
Java/Node-free double bootstrap and compiler identity verification pass. Full
receipts/logs are retained under
`results/m12-eval/runtime-macro-service-after-owned-form-api`. The missing
inline forwarding method and test-only owned-form API compile failures remain
archived. Public eval, request-scoped callback installation, real Vars and the
complete primary/body acceptance still remain required.


## Public eval entry and current native boundary (2026-11-24)

The ordinary public eval function now decodes actual nominal form data and
compiles it through the persistent native compiler. It reuses caller binding
cells, the isolated Macro authority, current dynamic namespace and math
context, and cumulative allowances. Function constants return the original
root. No prior source is replayed and form data is not printed and reread.

At ab2caa9b, the strict reentrant compiled-macro group returns0 (160.200s
including build). The complete clojure_eval target returns101:4 tests pass
and1 fails. All15 independent controls match in both Runtime and Macro,
including retained post-GC comparisons. Runtime fixture controls for embedded
globals, qualified globals and dynamic namespace selection match; the group
then fails at ordinary-var-result, leaving later fixture controls unexecuted.
All recorded inputs remain unchanged. Receipts and complete logs are under
`results/m12-eval/public-eval-context-after-target-correction`.

The preceding launch used an incorrect target path, was intentionally
interrupted and returned-2 with unchanged inputs. It establishes no native
pass; its receipt, partial log and target-correction explanation are retained
under `results/m12-eval/public-eval-function-and-namespace-context`.
Real live-cell Vars, top-level failure/effect boundaries, all34 unchanged
original eval assertions, capability/budget/GC regressions and full validation
remain required before claiming issue515 acceptance.


## Native live Vars and complete eval body (2026-11-24)

At6a82a270, all10 fresh Clojure1.12.6 live-Var controls match in both
Runtime and Macro, including independent retained comparisons after GC.
The focused Cargo command returns0 (159.481s including build). Canonical
phase binding identities intern actual nominal Vars; their compiled getters
read live public/dynamic cells without constant substitution. Definition
results, redefinition, metadata, callable references, eval identity, failed
initializers and const-root reads are covered. Source/primary verification is
`python3 results/m12-eval/verify-live-vars.py`; all22 inherited IFn signatures
remain explicit in the retained source adaptation.

The complete unchanged original34-assertion eval body also returns0
(11.231s including Cargo):29 host matches and the original five opaque
function/Var judgments at ordinals15–19, with no failed, missing or skipped
rows. Exact numeric ordinals5–7 are independent host matches. The full
public target still returns101 (6.916s):5 tests pass and1 fails at the
primary do-prefix-effects-before-compile-error control. Its observed prefix
trace is empty; the primary requires[:prefix]. Later fixture rows remain
unexecuted. All recorded production/test inputs and HEAD remain unchanged.
Receipts and complete logs are under
`results/m12-eval/live-vars-after-canonical-descriptor`; earlier compile,
metadata-binding, special-form and descriptor errors are preserved separately.

This extends focused execution evidence, not a complete eval/numeric issue or
full-suite acceptance claim. Macro-expanded top-level do boundaries, additional
namespace/capability/budget/GC regressions, complete-capture preservation,
bootstrap, exact full workspace validation, independent review and final CI
remain required. The next durable recorder also hashes every tracked fixture
and evidence input; untracked result outputs alone stay excluded.

Reconciling parent974ed7e7 changes ancestry only: its tree equals the
previously reviewed parent8d81cf3c, and the merge index equals6a82a270's
complete tree before these evidence/doc additions.


## Existing host and bootstrap regressions after live Vars (2026-11-24)

At6be4ee43, `run-installation.py after-live-vars-existing-regressions
--macro-service-only` returns0. All7,702 recorded inputs, including tracked
fixtures/evidence, and checked-out HEAD remain unchanged. The focused service
test passes (134.052s including library build), followed by14 macro-library,
nine inline-module,37 session-library, four observation and four original
bootstrap execution tests:68 distinct regressions. Command times are7.433,
36.812,11.081,3.205 and69.960 seconds. Java/Node-free double bootstrap and
compiler identity reproduction pass. Receipts/logs are retained under
`results/m12-eval/after-live-vars-existing-regressions`. The separately recorded
public do-prefix failure remains101; this gate does not relabel it or establish
full eval/numeric acceptance.


## Public eval top-level compilation boundaries (2026-11-24)

At3d51d8b5, the top-level boundary gate returns0 with all recorded inputs
and HEAD unchanged. Eleven fresh primary controls match in both native phases
and after GC, including macro-produced/nested do, lexical-body precompilation,
runtime throws, definitions before later errors, once-only macro expansion and
reentrant compiled Macro eval. The focused command returns0 in158.281s
including build; the complete public target returns0 in9.156s (all7 tests).
All24 original public controls now match in both phases. The complete original
34-row eval body returns0 again in10.944s:29 host matches/five original opaque
judgments; numeric ordinals5–7 remain host matches. Archives are under
`results/m12-eval/top-level-do-first-native`.

The native compiler expands only the root to select Clojure Compiler.eval's
top-level do boundary, then compiles and executes each unit through the ordinary
pipeline against the same live cells. Lexical bodies retain whole-form analysis;
no operator or expression body is interpreted on the host. Namespace mutation,
capability/budget/cancellation checks and broader capture/validation remain
separate acceptance gates. New pinned namespace controls are under
`results/m12-eval/namespace-primary`; they are primary evidence only.


## Eval dynamic namespace context (2026-11-24)

Atd2d3e7fc, all15 fresh namespace controls match in Runtime and Macro and
after GC (162.458s including build). Eval inherits the actual caller frame;
scoped bindings restore, unscoped namespace directives persist to later inputs,
and namespace prefix changes survive later compile errors. Dependencies retain
ordinary load frames, and native eval policy is cleared before publishing
catalogs. The complete public target returns0 with all8 tests (11.372s),
rerunning all60 primary controls in both phases and the reentrant path. The
complete original34-row body returns0 again (10.848s). All recorded inputs and
HEAD remain unchanged; archives are under
`results/m12-eval/namespace-after-rooted-checkpoint`. Earlier failing context
and compile attempts remain archived without success reclassification. Capability,
operation budget/cancellation, full capture, broader regression/workspace and
review/CI gates retain their separate requirements.


## Public capability, allowance, cancellation and retained functions (2026-11-24)

At88d244c0, the complete public target returns0 with13 tests (38.388s
including build), and the complete original34-row body returns0 again
(6.322s). A real AOT component has no host imports and reports the missing
compiler capability. Repeated eval and eval of a compiled infinite loop exhaust
the caller's original200,000-unit allowance in both native phases; a watchdog
would fail an accidental refill. Runtime and actual reentrant Macro eval cancel
after their compiled lexical bodies reach independent signal-only observers;
both remain usable and the Macro lease returns. Returned public-eval functions
survive GC and public macro-owner teardown while reading live redefined roots.
All recorded inputs and HEAD are unchanged. Archives are under
`results/m12-eval/public-eval-capability-budget-first-native`.

The standalone primary verifier checks all60 ordered expressions, observations
and pinned jars plus the complete eval adaptation. Source provenance metadata
for the retained char/min-key/some-fn/assert/random forms is refreshed after the
eval/Var additions; their actual adapted forms and primary fixtures are unchanged.
These metadata updates do not reclassify old frozen receipts. The new broader
gate hashes every tracked input and keeps a separate strict capture review before
any baseline update.
