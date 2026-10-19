# Native feedback and bounded repairs

On48de55e, [CI39281973631](https://github.com/bobby/suss/actions/runs/39281973631)
completed Java/Node-free reproduction, compiled identity verification and all4
compiled_bootstrap tests (0fail,62.86s). CLI library36PASS/0FAIL. Compiler library
447PASS/13FAIL; no aggregate CI or issue acceptance is claimed. Two normal tuples
also equal all four shipped files exactly; independent source/integrity review
confirms this.

[Raw logs, source patch/postimage hashes, reference and independent reviews](m4-pr263-native-feedback-repairs-2026-10-19.tar.gz),
archive SHA256 `2fd5a901c47092e743b4b2a0b132ebffb5f1f73dfdecef89bef2cd9bd9a93b0d`. Integration0/3 logs are only those
partitions, not the complete integration union. Later unexecuted cases in failing
tests are not passes.

The13 compiler-library failures comprise4 source-root fixture selectors,1 meter
diagnostic-span assumption,1 unresolved-reader-prefix fixture,1 invalid universal
construction-hash assumption,4 compilation-fuel failures and2 actual retained-key
identity failures. The latter remain under repair; no assertion was relaxed to
hide them.

## Source changes

- Plain function and resolved-source-symbol ASTs now use their explicitly proved
  literal `:env` keys, matching analyzer.cljc2362–2368/4108–4116 at
  c4295f303100bbf5afac449242d30bca1126f1a1 (EPL-1.0). New controls are prepaid.
  Both-phase live hash/GC/restoration/source-cold/unknown controls are authored.
- Source-root tests select actual entries and strictly require literal `:meta`
  versus retained reader `:doc` key domains. Owner/poison/source/callback/phase
  assertions remain. Native reader selection resolves the unchanged quoted
  arglist fixture before HIR; raw-prefix rejection is retained.
- Meter-failure tests require the exact error message and either the injected
  span or this fixture's exact metadata source span; both routes must occur.
  Every injection must fail, preserve inputs and permit full retry.
- Collection hash observations save initial memos and construction-event counts
  before one explicit actual-receiver hash. This permits legitimate small maps
  with zero construction dispatch while requiring numerical initial memos and
  one additional live event; restored-method events remain empty.
- Five semantic macro fixtures now use the established10B compilation allowance
  (formerly default10M): collection, control, quote/metadata, named dispatch and
  local reference. Four had actual fuel failures; collection had an assertion
  failure and now performs additional explicit witnesses. Caller Runtime
  allowances and dedicated default/exhaustion/recovery controls are unchanged.
- Pair-only metadata normalization now uses a smaller carrier without reader
  origin/coordinate payloads. It shares the exact association/ordering algorithm
  with the tracked reader view. Allocation/relocation fees use actual sizes;
  tracked-reader fees/proofs and default limits are unchanged. This removes
  work/payloads rather than reducing their rates. The new equivalence test covers
  both normalization modes. It is NOT proof that default core17 now passes.

## Validation and remaining limits

Independent source reviews approve each bounded contribution and their exact
postimages. Rustfmt parsing and `git diff --check` pass. No new local Cargo/VM
producer was launched while the exact parent workspace was live. All new native
regressions and repaired failures require actual execution. Source changes require
normal regenerated bootstrap output before fresh committed-head execution.

Retained pinned JavaScript reference ran in two fresh Node processes with115
input hashes checked before/after and identical output. It confirms initial
`:env` memo -1815813235 for function/local/global ASTs. The global map makes zero
analysis-time dispatches for that receiver; explicit hash enters numeric. Failed
authoring trial retained. This is not a fresh reference build or native acceptance.

Actual macro transport still splits outer/inner source-key identity through
legacy metadata interception's separate materialization cache. Plain Graph
sharing passes do not discharge that failure. The completed type-value/occurrence
projection and genuine cold/warm reuse coherence remain separate pending work.
Several integration fixtures also exhaust plain core-import native analysis at
10M; the17 strict default controls remain unchanged, and the compact carrier's
adequacy is unproven. Do not promote, close issues or claim whole-core conformance.
