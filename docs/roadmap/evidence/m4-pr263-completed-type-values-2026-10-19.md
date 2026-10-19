# Completed type metadata values and explicit hash witnesses

Source evidence only for the new changes. Parent/source base:
`32077d435c7c7716ceb21108ff684728740c55c2`.

[Retained source, independent reviews and raw native feedback](m4-pr263-completed-type-values-2026-10-19.tar.gz),
SHA256 `3f934a0b54cf60f069a0b74f5e57dc3af8f5c5e16427684597700e7cba103906`. The archive includes a hash inventory.

## Native feedback on the unchanged source base

CI39290009054 foundation118188066802 reproduced both normal bootstrap phases
twice byte-exactly against the four shipped files. Actual compiled_bootstrap:
4PASS/0FAIL (63.90s); CLI library36PASS/0FAIL (331.40s); compiler library
456PASS/6FAIL (1138.25s). Seven formerly failing tests now pass, as do the new
function/reference env-key and pair-only metadata regressions. Foundation still
fails and does not establish downstream library/documentation coverage.

Terminal integration shard3/job118188067008 has145PASS/31FAIL:29 core-loading
native10M exhaustions, including all17 unchanged core_import controls; one
namespace compilation Wasm-fuel failure; and one callable namespace retained
child identity mismatch on same-revision calls2/3. Both namespace failures
reproduce in both caller phases. Core setup failures execute no later semantic
observations. Pair-only normalization has not restored default core adequacy.

## Source policy

The native completed-declaration owner retains original declaration/definition
source Forms, source provenance and flat callable/inferred values. A type
revision carries one collapsed last-writer plan with per-value original source
owners. It does not retain a predecessor Info/HIR/environment chain. This is
source-sized storage, not a fixed-small-memory claim. Completed sources use
actual native ownership and complete input validation; equal copied or changed
foreign owners cannot supply value authority. Provisional or unproved sources
keep an explicit legacy/unknown fallback.

The value overlay implements fresh type :meta absence, retained prior metadata,
explicit current map/false/nil overrides and prior function/doc/tag behavior
from the [eight-case pinned reference](m4-pr263-type-meta-value-reference-2026-10-19.md).
The same native value writer serves ordinary and cold type views; existing
ordinary validation still runs first. Eager source capture and the duplicated
validation/value projection have real additional costs. Their default-fuel
adequacy is unmeasured; no rates, defaults or strict controls are changed.

Authored regressions cover eight reference cases cold/current-first and
warm/prior-first, both caller Stores, GC, live hashing/restoration, source-owner
negatives and typed fuel/IRQ/retry. Fourteen ordinary projection comparisons
retain a separate legacy control writer. The internal current-meta helper
identity comparison does not prove public original macro-frame map identity.
Separate legacy subroot materialization still requires a genuine coherent
source-child repair.

## Test-observer correction

Three AST families now reach boolean assertions after the earlier10B
compilation-only calibration. Their shared observer incorrectly demanded a
constructor callback for every key, including keys in small maps that need no
construction hash. The observer now saves the actual receiver memo and earlier
events, explicitly hashes that receiver, and requires its correct return and
memo plus exactly one additional numeric-entry callback. All earlier receiver
entries must remain numeric; they may include earlier explicit witnesses on
shared key objects. Canonical restoration requires zero receiver events before
and after the witness. Required operations, named-function witness, source
programs, cold sentinels, caller limits, phase coverage and GC checks are unchanged.

## Review and execution limits

The full type-value patch143b63e9 and composed patchf4860e4a have independent
source/composition GO. Strict zero/one typed OutOfFuel controls were repaired
before that review. The observer patch3684f649/postimagee909b015 has independent
test-only GO. All13 source postimages are recorded exactly in the archive.
Rust parsing and apply checks pass; retained pinned cljs.tools.reader parses
the entire revised observer. Those are not Rust typecheck or macro execution.

The separate parent workspace producer remains live on66a55ce. No local Cargo
or Suss VM ran for this publication. Four shipped bootstrap files are retained
from320 and are not yet regenerated for these new sources. Generate both
normal phases twice, independently bind/adopt their complete tuples, then run
Java/Node-free bootstrap validation/execution and the focused regressions before
the exact full workspace baseline. Current source acceptance is unproven.
