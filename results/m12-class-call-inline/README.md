# Inline descriptor-call rejection

The complete partition6 CI log at reviewed head22c0a3f2 found a genuine call-site regression: CheckNumericCallee introduced a cold argument array, an invoke import and extra helper calls. `before-ci.log.gz` preserves the actual failed job137379960856 from run45078123032. The original `portable_calling_convention` assertions remain unchanged.

A physical DESCRIPTOR is neither a CLOSURE/UNIVERSAL nor a callable collection. Source type constructors and protocol values use closures instead. The guard now uses that structural proof to throw before arguments, without a canonical lookup/helper call. It imports the existing canonical not-callable descriptor (runtime global2) read-only, shares a pooled short message, and constructs a fresh error with nil data/cause and UID0 through the existing language-exception tag. The runtime only exports an existing root; its globals, shared types, layouts and ABI10 are not relocated or changed.

The fragment counts the new readonly global after any protocol epoch and before its own definitions. Existing imported slot/code indices stay fixed; owned slots, caches and literal roots include the extra import. The common call path remains an inline descriptor test. Its method contains no new argument array, runtime invoke, helper call, cast or field read; its sole normal helper remains the entry-cache lookup. The four strict calling-convention tests pass unchanged, as recorded in calling-convention-direct (exact executable command/hash and raw output).

`oracle.py` obtains one new fresh Clojure1.12.6 control: separate class-call failures have different error identities and nil ex-data. It feeds the existing Runtime/Macro/actual-AOT class matrix in addition to all previous fixtures, which remain byte-identical. The regression guards against reusing a shared error object while pooling its message. This does not claim general noncallable-call or JVM exception-class/message parity.

`run-focused.py inline-descriptor-throw` covers the unchanged calling convention, exception constructors, source numeric classes, actual AOT, nonescaping closures/vectors, unboxed codegen, compiler catalog, record budgets and prior integration targets. `run-gates.py` covers surrounding reader/numeric/ABI/printing/Java-free bootstrap/manifest/workspace checks. `run-suite.py` and `audit-suite.py` retain the full vendored capture and its exact comparison with the existing reviewed baseline. Each terminal receipt qualifies its own source inputs; a receipt that has not completed is not a pass. Original failed results remain evidence.


Terminal evidence:

- inline-descriptor-throw:0,86 tests/12 targets, unchanged source and fixture. The calling-convention file/assertions are unchanged.
- gates:all eight commands0,227 tests (4 printer/80 reader/51 numeric CLI/88 compiler/4 bootstrap); Java/Node-free double reproduction, compiler identity, both manifests and workspace test-target compilation pass. Inputs unchanged.
- suite-commands:build/full capture/lock0, source unchanged. suite-audit retains4,247 raw rows and exactly3,739 host matches/61 guest judgments/459 failures/2,680 unexecuted/90 failed namespaces, with zero changed assertions/namespace diagnostics and no reference/baseline/skip/count-guard rewrite.
- independent-final:exact fresh command/output/exits for24 JVM controls and23 applicable current-CLI rows; the separate multimethod control is covered by its existing macro-aware target. It additionally retains two distinct errors/messages/nil data across20,000 escaping vector allocations with a1MiB initialGC heap. No observed GC event-count claim is made. Archive SHA256 before compression:87d4dc3ab016b5a65ccc2c499efd5450419f789555ff5bf18c214e291fc6244e.
- final-input-diff:documentation-only changes after the frozen runs; production/tests/fixtures match. Earlier unchanged primary fixtures and historical performance receipts remain qualified to their exact recorded inputs.
