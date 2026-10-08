# Complete map/filter draft evidence

The pin and source/patch identities are in the standard core-import manifest.
`files.json` binds the retained files and explicitly lists anything missing at
capture. Full workspace baseline and final-head CI are still required.

The final native gate passes four tests in Runtime and Macro phases, covering all
41 closed cases and GC/capture/raw-sharing assertions. Bootstrap pairs reproduce
byte-identically twice with Java/Node absent; identity invalidation and four
bootstrap tests pass. Core imports pass 17 tests; affected suites pass 2 chunk,
4 lazy and 3 reduction tests. Python passes 235 tests. Fresh pinned primary
observations match 41 cases. Logs report commands/build paths from execution;
the handoff and runtime document record reproducible commands.

Original incorrect expectations and actual primary observations are retained:
31/32-element vectors demand one element initially, lazy throws retry, and live
first runs inside both the transformation and LazySeq.-first. Single-arity
replacements of upstream multi-arity functions failed pinned entrypoint lookup;
matching arity shapes pass. Before-import map resolution and old-REPL reload
failures remain failures. The affected-suite target selection typo rejected
before semantic execution; the corrected suites pass.

The two CLI result files total 41 complete-source probes. They exited zero with
no stderr and matching closed printed values; they do not replace the lossless
FormBridge/native Wasmtime assertions. The finite 100M focused corpus allowance
is not a measured default-budget guarantee. Original M4 scope remains open.

The baseline at5f19cf8 exited101 on scalar async-import fuel exhaustion.
Exact testbinary reproduced it. Diagnostic10M measurement (notacceptedbound)
retains initializer1711536, calls78296/78258/78258/78258, GC0, polls2/4/6/8.
Total2024606 exceeds oldshared2M. Original testsource restored; initialization
stays2M and one additional2M is shared across allfour calls. Assertions
unchanged. Focused fixedsuite/review/finalhead baseline andCI pending.

Fixed focusedsuite50519 terminal0:8passed/0failed/ignored/filtered23.27s.
Independent fixreview confirms measurement/boundedbudget/assertionsunchanged.
Final-head fullbaseline andCI remain pending after commit.
