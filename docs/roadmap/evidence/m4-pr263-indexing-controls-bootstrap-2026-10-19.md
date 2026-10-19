# Normal bootstrap tuple for indexing-reference controls

Source: `a1d3c61f6c2611a21893d1bcb1ae189b1563eb0a`.
[CI39308835115 foundation118253018842](https://github.com/bobby/suss/actions/runs/39308835115/job/118253018842)
completes both normal `scripts/verify-bootstrap.sh` generations. It then fails
comparison against old shipped files before bootstrap or library tests execute.
The two generated copies of every phase output are identical. Exactly the four
first-generation outputs are adopted without manual fingerprint edits.

[Artifact12214683194](https://github.com/bobby/suss/actions/runs/39308835115/artifacts/12214683194)
ZIP SHA256: `9983d31446c408db9b5415675543e2928a866d752ce049cc90d776da8d4639cf`.
Synthetic checkout `a8151e0e91948a13b871994510a09fabdcefb76d` is clean; its tree
`38c8afeef4da5e65807fd1d63ba7093adfbadb72` equals the actual branch tree.
Compiler source fingerprint:
`2a05f7190c9910ef0c98c2d6275ae134991ff29c351d6e1fd93886cba2afb9e6`.

Independent review verifies all204 Git source inputs,9safe ZIP members, all8payload
hashes/sizes and4equal pairs. Both phases' executable non-custom sections match
the previous shipped tuple; catalog, ABI, target, flags, cells and other manifest
fields retain their values. Two actual existing compiled build scripts independently
report the expected source fingerprint. After adoption, the actual existing compiled
build-script identity probe passes, including checkout independence and embedded
library/numeric/WIT mutation controls. That script invokes no Cargo or Suss VM.

| Output | Bytes | SHA256 |
|---|---:|---|
| first/runtime.wasm | 2108122 | `fa446910507ea39e04352fa089bf39b0feb560a1afa0396e1b79f076c837cb1f` |
| first/runtime.json | 47413 | `1b6fe184f23b163a030007da1911f54d63e7480ff1cc76e1163409e737eb78ed` |
| first/macro.wasm | 2106643 | `0ad0ef5f83c835c3dd35025e7ba2b898cf2a3f5fbedc751ede46309d6e894b0a` |
| first/macro.json | 45875 | `7c243a2fd92a9f09ed25bf797a75e7d63447dc64136f8c3786cf5d5612ddf09f` |

Archive `m4-pr263-indexing-controls-bootstrap-2026-10-19.tar.gz` SHA256 `26f69b8bba63cff0eb1ca413d7df4ecd730a13f19ed6f01cf7b78aadb894d57a` contains 19files: exact both-generation
payloads, source/API receipts, raw failed-comparison log, independent verification
script/report and identity-probe result. Generation proves bytes and source binding;
it is not artifact validation/linking, bootstrap execution or corrected-test acceptance.

No Rust production/test source changes in this adoption. The prior commit's two
reviewed test corrections remain intact. User10M defaults, caller Runtime, strict
core17/complete one-scope source-child and original pinned-map200M/200M controls
retain their values. The current known failures remain explicit in the preceding
[reference/control evidence](m4-pr263-reference-qualified-controls-2026-10-19.md).
Unintegrated binding-memo/controller candidates are outside this source tuple.

Next: committed-head normal `sh scripts/verify-bootstrap.sh`, focused original
completed-value/literal/type-value/reader/source-root/core-import/source-tags and
pinned-map controls, then the exact workspace baseline, independent final review
and final-head CI. Parent66a55ce's live exact workspace producer is separate and
unchanged. No concurrent local Cargo/Suss VM ran for this verification/adoption.
