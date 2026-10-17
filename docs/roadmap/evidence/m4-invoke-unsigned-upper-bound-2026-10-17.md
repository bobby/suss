## Unsigned closure arity bound: commands and evidence (2026-10-17)

The central invocation helper now compares unsigned argument length directly
against the maximum arity. Its former `max != -1` conjunction was redundant:
`-1` is u32::MAX and no unsigned length exceeds it. Six pure Wasm operations
are removed; minimum/type/argument-array checks, canonical error payloads,
environment validation, call selection and argument allocation remain intact.

`python3 run-invoke-unsigned-upper-bound.py` regenerates the normal four
bootstrap outputs, then executes 15 closure tests and 86 runtime ABI tests:
101 pass. The new raw-closure table checks 13 explicit metadata masks at four
argument counts after GC, including fixed/variadic/negative boundaries. The
complete unchanged source-child fixture also passes, all220 ordered labels
and results matching source71. Each phase retains the unchanged10M allowance
and finishes129,753 versus105,471: 24,282 actual units removed per request.
No whole-record/CoreEnv savings are inferred. The two record witnesses and
reduction witness still exhaust caller &env transport before criterion results.
This focused run is105 executions:102pass/3fail, zero ignored,2,986inputs stable.

After that producer terminated, the reviewed when-let integration correction
changed only its expected diagnostic phrase to include the existing compiled
invocation stage. Every invalid form and recovery assertion remains. The exact
historical fixture is retained; provenance requires one explicit replacement
and byte-identical positive guest input. Historical oracle inputs and receipts
are unchanged. `python3 run-invoke-unsigned-upper-bound-followup.py` passes
provenance plus20Rust executions: corrected invalid1, unchanged pinned bitwise1
(70observations), CORE17 and full pinned map1. Map comparison verifies all69IDs,
62independent host passes/seven opaque guest judgments, zero failures/skips/
unexecuted. All2,987follow-up inputs are stable. Runtime compiler sources and
bootstrap bytes are unchanged across this test/provenance-only correction.

The separate source71 preservation producer is now terminal:105pass/0fail/
0ignored selected executions. Its remaining21original collection tests all
execute; with the three source71focused fuel failures, original24 is21pass/
3fail/0ignored/0unexecuted. All2,986source71inputs remain unchanged. These are
explicitly prior-source results, not a fabricated rerun on the new ABI source.

Two host attempts to compare the bootstrap as if it contained kernel invoke
were rejected (unsupported import parsing, then absent invoke export).
Bootstrap imports that helper; no kernel bytecode comparison was established.
Those host failures remain labelled separately and do not change guest verdicts.
Actual saving above comes from source-child execution, not byte-count inference.

Evidence: [thin supplement](m4-invoke-unsigned-upper-bound-2026-10-17.tar.gz), SHA256 `6455ee46741981db166cfc593711236681ea584008c39ed6e202803dd9f5a9f5`.
It resolves common frozen source blobs against the exact retained
[base archive](m4-boxed-integer-bits-checkpoint-2026-10-17.tar.gz), SHA256 `266d1ce35b4014cbe9eb67fdcf20220cd2dcec0784f0aecb34ef7412e7cdc88c`; new blobs,
all raw run logs, source maps, commands, actual map capture, corrections and
initial failures are retained. `BASE-AND-SOURCE-RESOLUTION.json` documents the
checked dependency; this supplement is not standalone without that base.
Independent source/execution review precedes publication. The required current
source full workspace baseline and final-head CI remain separate pending gates.
The next substantive repair candidate is repeated HIR subtree copying during
source-AST preparation; no benefit for that candidate is claimed here.
