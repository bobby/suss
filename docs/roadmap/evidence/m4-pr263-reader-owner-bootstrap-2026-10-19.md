# Normal bootstrap regeneration after reader-owner and test fixes

Run [39281448728](https://github.com/bobby/suss/actions/runs/39281448728),
foundation job118156503864, executed both normal Java/Node-free generators on
source32ef328351f823cc9be7bb8c2a2ff3f066a450c2. Its actual clean PR merge checkout
1ed625a08f8c8341a8357fe6d8f09e8fa1bd7df7 has the same tree as the branch:
f249ffd42f0bb3f9591f11409b49277f338979dc. Both complete output tuples are
byte-identical. The unchanged shipped comparison failed at runtime.wasm
byte2107602; later execution gates did not run on that generation job.

The retained artifact12198754254 ZIP SHA256 is
`16d2b5c529ec8c441c1c4b08a7c9ba21f9bd448c729ab6c48cbb09cfe796c8fd`.
All nine allowed archive paths, eight output sizes/hashes, four reproduction
pairs, API run/head/merge/tree and clean receipt were checked. Both manifests
retain the complete catalog cells, source/dependency hashes, ABI, compiler
version, target and flags. The actual existing compiled build-script executables
compute matching current compiler fingerprint:
`969e2858df02dfb85f44752e81a71cbe87209b6d0cdfbf253f0e511fc713e183`.
No Cargo or Suss VM was run locally for this fingerprint check.

| File | Bytes | SHA256 |
|---|---:|---|
| runtime.wasm | 2108122 | `787683d7e0d3217870b13180054d03b6315e142757ddc3aba7fe74e78adcbd00` |
| runtime.json | 47413 | `f806be6998077d88f59fd144f989744caf94af3a4d55d8bb96f21d2dba486c5b` |
| macro.wasm | 2106643 | `802f04c9f6633b21b28bf9655b46d01bb59214d908e79fe64713b7b2e60697b1` |
| macro.json | 45875 | `d83c180fe202eb93d3ca41c7270f952a2ca31b2bba9e4fe92abb8e2987ec0f4c` |

Both phases have byte-identical non-custom sections to the ca8c074 tuple, which
passed all four actual compiled_bootstrap tests in run39280069179. This byte
comparison does not validate the new native compiler source or its regressions.
Two independent reviews authorize copying exactly the four actual generated
files; both independently recomputed the compiler fingerprint from200 immutable
Git inputs. Those four files are now adopted. Normal reproduction, identity and
bootstrap execution must pass again on the new committed tuple. The corrected library-test compilation and newly added
borrowed-reader tests require actual native results. The exact parent workspace
baseline remains separate and must not be restarted or credited before terminal.

[Integrity/source/review receipts](m4-pr263-reader-owner-bootstrap-2026-10-19.tar.gz),
archive SHA256 `3a1f3c1e71836080d1743d4137b25568e0cb468885b202b8534a41030751ad51`.
