# Private factory-slot fixups and completed fuel measurements

The proposed source extends `4f48ea6bd7d2e198b9dd82086e4abf4e118e7a7b`
with six Rust paths and four normally regenerated bootstrap outputs. It ports
`4ab8a76a48469b1138bba1dc14ad7deff5afcad6` with an allocation-extent correction
and stronger regressions. The earlier scratch run, 4f CI/baseline, 2B diagnostic,
and new slot-log run are separate executions, not a combined acceptance total.

## Private slot-log execution

Fresh callback-free private constructors record final physical placeholder slots,
including normalized copies. The builder uses the log only for a complete,
covered route; unsupported routes retain the old full walker. It validates all
recorded locations before writing. A write-phase budget failure may partially
patch an unpublished private graph, which must be discarded. The log grants no
source or memo authority, and does not skip callback-driven source checks.

The original module's allocation precharge now includes `RefCell` borrow state:
`size_of::<RefCell<Self>>()`, rather than `size_of::<Self>()`, plus the existing
Rc header charge. This adds eight actual allocation bytes on the tested host;
it does not lower a tariff. Original source and the exact adaptation remain
archived. New tests cover a later poisoned slot leaving earlier slots untouched,
explicit incomplete-log fallback, and every integer fuel allowance through the
measured two-slot capture/apply costs. IRQ coverage is operation-entry only,
not exhaustive mid-operation injection.

Normal bootstrap generation and identity verification pass. Focused execution:

| Selection | Result |
| --- | --- |
| Private fixups, including slot-log | 11 passes |
| Reuse/callback/rebuild/fallback | 6 passes |
| Original source tags | 2 fuel failures |
| Original CoreEnv | 1 fuel failure |
| Original collection source | 1 fuel failure |
| Pinned map wrapper | 1 pass |

Total: **18 passes, four fuel failures, zero ignored**; every stage executed.
All 3533 frozen inputs match Git4f or the exact six-source/four-bootstrap
postimages. The new untracked module was included before the first Cargo run.
Only the four bootstrap outputs changed during generation; later inputs stayed
unchanged. The controller drained and retained the intended source overlay.

Measured small-graph capture cost is458 units and apply cost176. Every lower
integer allowance fails and fresh retry succeeds; one apply cut observes a
partial private write prefix, with the public source unchanged. These are small
native fault tests, not whole-operation complexity or exhaustive IRQ proofs.

Independent map comparison preserves all69 ordered IDs:62 host-decided passes
and seven guest judgments, with no failure, skip or unexecuted assertion. Those
seven judgments are not independently decoded host-value passes.

Compared with4f, original metadata marker order is retained. The cold CoreEnv
checkpoint delta is0. Collection has2,202,066more fuel at its final common
checkpoint, with an earlier91-unit adverse delta retained. These graph groups
are not independent completed outer requests; required failed tails are unknown.
Collection still fails during invocation, now later at851..882 rather than
709..738. Source inference still fails, now later at caller &env382..414 rather than
324..356; invocation-tags still fails at388..413. Neither complete source-tag
criterion is satisfied. Historical4ab or later-branch passes are not current
acceptance evidence. No wall-time improvement or general absence of regression
is established. Quotas, rates and fixtures are unchanged.

## Earlier 4f results

The scratch candidate's separate focused cohort is28PASS/4fuelFAIL. Its matched
CoreEnv checkpoint gain is371857; its collection checkpoint loses7296 units.
Those observations are not added to the slot-log cohort or treated as whole
request costs.

Exact4f CI39156456734 is terminal failure. Foundation:400 Rust passes,
zero failures, one existing ignore, and353 Python passes. Integration:
124 passes/18fuel failures,31/251targets completed220unexecuted. All18 failure
names/stages/spans match d38; no previously passing observed name now fails.
Unexecuted coverage prevents a global regression claim. Nine synthetic checkout
trees match4f exactly; checkout equality is not an input freeze.

The exact command `cargo test --workspace --locked -- --test-threads=2` at4f
is terminal:exit101 after3556.25seconds,86passes/two fuel failures/zero ignored
across17harnesses. Fourteen of251integration targets completed;237 and all
doctests were not reached. Failures are collection invocation709..738 and
unordered caller &env510..531. The frozen inputs match Git4f and receipt
`changed: []`. This baseline predates the slot-log overlay.

## Separate 2B diagnostic on 4f

Only the test's Macro allowance changes to2,000,000,000; caller defaults and
all original assertions remain unchanged. First Runtime expansion trace:

| Boundary | Remaining fuel |
| --- | ---: |
| start | 1999999819 |
| provenance | 1999996363 |
| caller-form | 1999996362 |
| caller-environment | 298930259 |
| arguments | 298924105 |
| invoked | 298714008 |
| decoded | 298713892 |

The completed start-to-decoded interval consumes1,701,285,927 units;
caller-form-to-caller-environment consumes1,701,066,103 and the transformer
210,097. The first projection's independent decode/GC/row assertions pass.
The second expansion starts with1999999804 and still fails caller &env0..36;
its completion cost is unknown. The second row and Macro caller phase remain
unexecuted. The entire Rust test fails,0PASS/1fuelFAIL, after269.12test seconds.
This is not a successful whole-program run, total compilation-cost measurement,
controlled performance comparison or revised default. All diagnostic inputs
stayed unchanged and its known test-file postimage was restored before the
slot-log candidate was applied.

## Archives

- [m4-pr256-raw-key-scratch-focused-2026-10-18.tar.gz](m4-pr256-raw-key-scratch-focused-2026-10-18.tar.gz) — SHA256 `3fadfbd82b1ab70f359432ef0538ab67afde3dcff13f09620d0d163b139105cd`
- [m4-pr256-4f-terminal-ci-2026-10-18.tar.gz](m4-pr256-4f-terminal-ci-2026-10-18.tar.gz) — SHA256 `b16ddf5b615533520253e269247c68f877e3398a9ad613b867c2bf4f0aae8241`
- [m4-pr256-4f-workspace-baseline-2026-10-18.tar.gz](m4-pr256-4f-workspace-baseline-2026-10-18.tar.gz) — SHA256 `6c07c82e7d5e2c9bbbde3e40adc33909ef279f1eab4a8af384bf3abc929b8eeb`
- [m4-pr256-coreenv-2b-diagnostic-2026-10-18.tar.gz](m4-pr256-coreenv-2b-diagnostic-2026-10-18.tar.gz) — SHA256 `d4804dd266f6d565c291e1f25a308c1e7dc4f5fbcae86e2da5a89529fdbecae1`
- [m4-pr256-private-slot-log-focused-2026-10-18.tar.gz](m4-pr256-private-slot-log-focused-2026-10-18.tar.gz) — SHA256 `f5dbb950e2a4b376a0bdae861f2bb2debe80071821c89985ad8af3d61b47f278`
