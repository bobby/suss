# Fixed prototype inspection arrays

Tested source `a09ed8731924f5fef754e687c85468d2bfa66b18`, based on `7b7d3a77d367f027ce1a1be80f2464045fbf122a`, on the separate coherent candidate branch. One Rust file and four normally regenerated bootstrap artifacts change. No production adoption is established.

Prototype inspection always reads fixed arrays of twelve or two values. Replace four temporary Vec backings with stack arrays. Preserve the initial count/type/length guards, ascending reads and their charges, RootScope lifetime and persistent owned fields. Prepay each stack initialization; remove only actual heap allocation/backing work. Fresh private origin queries, source-constructor checks, all twelve reference comparisons, nested prototype/property checks, literal UTF16 checks and parent storage validation remain. No cached verdict crosses a callback, and no rate/quota changes.

Normal bootstrap and compiler identity pass. All3670 tracked post-bootstrap hashes bind to the tested commit and stay unchanged through both captured drivers. **34 distinct Rust tests execute:26PASS/8fuelFAIL/0ignored**:

| Selection | Pass | Fail |
| --- | ---: | ---: |
| Prototype origin guards |4|0|
| Existing prototype buffer/audit preservation |4|0|
| Complete source-child fixture |1|0|
| Coherent guards and legacy retirement |12|6|
| Public callable/namespace identity |0|2|
| Pinned CLJS value/effect and decoder checks |4|0|
| Original pinned Jank map |1|0|

All six original default-budget callable requests complete the same51 roots and still exhaust constructing boundary26 before export/finalization/publication. Matched completed-prefix work falls by183804–183816 units per request, including stack initialization charges. This measures removed allocation work, not whole-operation savings or an identity fix. No rows decode. Four repeated-request error strings now include guest backtraces; first-request errors remain equal. Preserve those raw differences: receipts are not equal, though their span25..75/caller-&env/fuel classification is unchanged. Namespace fails during the first lexical call's caller-&env transport46..82 in both phases; its required rows remain unexecuted.

The same six coherent tests fail from fuel exhaustion: four during setup/materialization, two in ORIGINAL reader-effect route controls before coherent arms/effect comparisons. No previously passing selected test fails. The complete source-child fixture retains all five variants/both phases and physical/memo tail. Existing prototype buffer exhaustive cuts cover scoped_prototype_fields; they do not establish exhaustive direct cuts of the changed generic N2/N12 helper. Existing public origin/fuel/IRQ/GC/poison/reset checks exercise the changed call paths.

Four pinned CLJS value/getter-effect cases match afterGC in both phases within one Rust test; three decoder adversaries also pass. The original Jank map independently retains69ordered IDs:62host passes/seven guest judgments/no fail,missing,skip, with existing200M allowances. No default-budget map or whole-suite claim; no fresh upstream oracle execution.

Exact commands, source/bootstrap blobs, freezes, positive/negative logs, parsers, controls and independent reviews are archived. Drivers use shared CARGO_TARGET_DIR, CARGO_BUILD_JOBS=1/RAYON_NUM_THREADS=1 and inherited RUSTFLAGS. Bootstrap: cargo run --profile test --locked -p suss-cli --bin suss-bootstrap -- runtime/bootstrap; identity: python3 scripts/verify_bootstrap_identity.py. Cargo selectors and trace/output overrides are retained exactly in run.py/preserve.py and their terminal receipts.

A separate unimplemented side-pool model, based on the previous source2d2275bc trace, is retained as negative evidence. Across482 captured snapshots/14943 Atom backing slots, a hypothetical48→32-byte Atom saves239088 backing bytes. Preserving2918 opaque owners under ordinary min-four/doubling pool growth would charge286560 allocation/relocation bytes:47472more before other costs. Layout is uncompiled and other costs unknown; neither implementation nor win is established. This is not current-source execution or authority to weaken ownership. Its raw trace is preserved in the earlier coherent-construction partition archive.

Full `cargo test --workspace --locked -- --test-threads=2` remains required and unexecuted on this candidate while focused acceptance fails. No candidate CI success, issue closure, promotion or merge. Next: remove measured enrollment/snapshot/prototype work while preserving complete validation and callback boundaries; ordinary opaque-pool growth is not supported by the retained model. Productionfd78441 CI is a separate failing tuple.

[Raw evidence archive](m4-prototype-stack-arrays-2026-10-18.tar.gz), SHA256 `f3714093142554bf3753691338023159f87ef11579df7b0b8b88d04428aa6383`; 39 members/38 payloads.
