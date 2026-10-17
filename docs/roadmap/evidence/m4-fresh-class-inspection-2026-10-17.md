# Fresh checked-ordinal source-class inspection evidence

Source commit `812b7257e3031cf1db5790a2e0d02d7128b53363` is based exactly on
`fe8f0ffe918e6beb9a5f5e1d7a89e4859ab92fbb`. It changes three Rust files and four
normal regenerated bootstrap artifacts. This is partial progress for #270,
#252 and #264, not issue closure or full M4 acceptance.

Class-result reuse is removed. The existing complete coverage/name scan supplies
an ordinal hint; the initial registry entry's name must match it, otherwise the
original full name lookup runs. Exact supplied Session/registry/root ownership,
constructor reference, CURRENT constructor-descriptor and full initial schema
snapshot checks still run for every input. Unknown/missing/duplicate/reordered
names and SourceArray's absent constructor retain their existing handling. No
ordinal grants authority. Class name hashing, class AVL insertion/lookup, retained
root Vecs and class Entry-vector allocation/growth no longer execute. Prototype
query reuse, collision safety, lifetime boundaries and nonshortcircuit late-error
handling remain. There are no rate/quota cuts, class-method shortcuts, GC identity
hashes or cross-callback caches.

Carver independently approved exact production patch `840fdf4a032d5de9dcd21d519328b57f83a54898a81ba9e3ba104e7f98dbbcda` before execution. The requested test-only strengthening requires a specific `SessionError::Trap(OutOfFuel)` at every insufficient allowance, including the allowance in failure messages. Final executed patch `802c1e65643f7b5d7fa78b87500789d8508dcfe5507bd03c391d2a58feeeba32` and original reviewed packet/delta are preserved. The detailed effect/meter audit is in `packet/DESIGN.md`; generated private constructor/environment adapters are reads/guards, with native error allocation/throw on malformed input. Existing dynamic checkpoint restoration is idempotent for these adapters. No user protocol/body is called.

## Commands and actual results

Actual commands, PIDs, raw logs and summaries are in `run/receipt.json` and
`run/command-plan.json`. Exclusively `memo-proof-cost-target`, jobs1/Rayon1/test1,
no RUSTFLAGS assignment. Order: compile lib no-run; NORMAL bootstrap BEFORE Session
tests; audit9; canonical11; constructor11; prototype4; unchanged default10M
sourcechild; CLI CORE17 in **suss-cli**; unchanged200M full pinned map test.

| Executed gate | Actual result |
| --- | --- |
| Compile / normal bootstrap | exit0 / exit0, only four allowed bootstrap outputs changed |
| Class audit, including three new regressions | 9 pass, 0 fail, 0 ignored |
| Existing canonical source-class/method tests | 11 pass, 0 fail, 0 ignored |
| Existing constructor inputs | 11 pass, 0 fail, 0 ignored |
| Existing prototype origins | 4 pass, 0 fail, 0 ignored |
| Default10M sourcechild | 0 pass, 1 explicit OutOfFuel panic, 0 ignored |
| Original CLI CORE17 | 17 pass, 0 fail, 0 ignored |
| Original unchanged full pinned map transport | 1 pass, 0 fail, 0 ignored |

Total: **53 passing Rust executions, one fuel failure**, nine stages including
compile/bootstrap. All2915 actual inputs remained unchanged after bootstrap.
All13 foundation commands subsequently exited0, including full Python351;
foundation freezes also match. Independent map comparison gives all69 ordered
IDs,62 host-decided passes,7 guest-judged (opaque-reference operands1/4/9/15/20/33/46),
zero failures/skips/unexecuted/namespace or test failures. Full rows and reasons
are retained. This is not69 host-decided passes. Both phases' ordered noncustom
Wasm section sequences equal fe8; that is artifact correspondence, not execution.

## Measured prefix gain and remaining failure

The executed V4 parent's entire2915-input tuple equals fe8 before this patch,
including all pinned source bytes. Both requests start at10,000,000. At matched
Runtime physical-child-check entry, remaining576042 ->1086622 (+510580); after
checks143480 ->981671 (**+838191**). That interval's actual cost432562 ->104951
(**327611 removed**). These are matched prefix/interval balances, never additive
nested savings or whole-phase costs.

Physical-input checks now return Ok with813814 remaining; the old default run
failed there and has no successful interval for comparison. The current fixture
continues through physical materialization/factory checks and reaches final-parent
construction. Its last final-parent-before marker is76024; parent-layout succeeds
with68420; a later materialized root20 returns with27618. The actual panic is
`source_roots.rs:950:63`, `standalone_with_layout` recording-bridge creation:
`Trap(wasm trap: all fuel consumed by WebAssembly)`. The helper unwraps before an
outer error/zero-balance marker. **No final zero balance, complete phase cost or
remaining full-cost gap is measured.** Final-parent completion/final validation
and the entire Macro-source half remain unexecuted; the full original test fails.

The runner's initial classification checks generic text `fuel`, which is too weak
because trace lines already contain it. Preserve that driver/flag, but do not use
it as authority. Independent root postprocessing requires the actual typed panic
and failed summary, records absent zero marker explicitly, and retains an earlier
overstrict postprocessor failure/correction. A preceding deferred-factory Host
error is an expected fixture negative, not the terminal panic. Own initial prefix
matching also included a changed Err->Ok outcome under the same label; the initial
JSON and correction are retained. Corrected matches require identical result
text and separate newly reached/changed outcomes. Root comparison is authoritative.

The focused fresh24-class loop spends2962, EXCLUDING ConstructionRecords coverage,
audit creation and setup. Old observed2517 included coverage: do not compare these
directly or assert a modeled3611 whole-region cost as measured. The old48 class
cost model motivates the change but is not this candidate's actual whole-phase
saving. Historical~3.276M gap on48 is not a measured current remaining gap.

No new full workspace baseline or final-head CI result is claimed. Independent
final committed-head/artifact review precedes checkpoint push to existing PR268;
a new exact-head workspace baseline is a separate planned run. No issue/milestone
completion, PR merge, whole Jank conformance or automatic next optimization follows.
