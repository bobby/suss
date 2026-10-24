# Shared reader-key controller source and build evidence

This publication proposes 34 Rust source postimages over 331175be9a287979153df05f10c7f0328d5d3df5. The exact source freeze is SHA256 79ec4632c4e5994e9633f7a74cf165a049e16e538f581f050af34be234a8160f. Source is not yet bound to a new Git commit.

The composition includes first-use shared reader transport, strict versus fresh-data checks, direct reader-key linkage, iterative evidence release, successful completed-owner binding memoization, prototype buffers, shared Arc definition syntax, reader property-value fixes, and genuine shipped-environment test setup. Independent composition review is GO_SOURCE_ONLY. This is not an execution or acceptance verdict.

Build history is retained: initial 31-file library no-run failed with 30 diagnostics; the next 34-file attempt failed with three diagnostics; mechanical API/import/lifetime repairs produced library no-run PASS in 20.01 seconds. That compiled binary listed 531 library test entries, not 531 passing tests. CLI no-run subsequently passed in 28.77 seconds, with zero CLI test bodies executed. These builds preceded the final two test-helper context repairs.

A six-test native ownership run then produced 4 PASS and 2 setup FAIL, without Suss VM execution. Both failures reached missing PersistentVector environment context before ownership assertions. The repaired Arc and memo helpers now use genuine shipped environments, preserving their programs and assertions. The corrected fixtures have not been executed.

On the final exact 34-file source, `cargo test -p suss-compile --locked --lib --no-run` passed in 24.95371462201001 seconds; raw log SHA256 is 7c0bda0e4fa3b0249bee0773bc5363a60ce7dffa2cc657f5d032e1628b957b98. This is compile/link evidence only. Two actual existing compiled build scripts returned source fingerprint c7da6b7f8237d35dddaf0731bfbef6918a7bd360cd15f374019b9a928bba1da6. Fingerprint input ordering uses Rust path components.

Normal two-phase bootstrap regeneration and identity verification remain required before genuine shipped-environment fixture execution. Existing bootstrap bytes are not evidence for this new source tuple and are not included as newly generated outputs. No new VM acceptance, public cold/warm identity success, map200M success, whole-workspace baseline, final-head CI, issue closure, promotion, or performance saving is claimed. Historical source failures remain historical; compile success does not resolve them. Parent PR256 was separately merged as 30f4787e89f60a6ceb77d853eda554e2e3b5b28d.

Raw source/evidence archive SHA256: `bcca02f7551029926955a317656089aaac4cc100d4beb0e49dcf3bdd1ad5f913`.
