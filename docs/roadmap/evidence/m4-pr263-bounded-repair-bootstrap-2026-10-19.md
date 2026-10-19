# Bootstrap after bounded native-feedback repairs

Normal Java/Node-free CI39289196572 generated both phases twice from4112c7c
(source merge3fdcfb721d63e95bab8ea8884c22952d1d61086e, clean tree
8a47f2b2e66aa4df89b198f769a65b77b17eb909). All four reproduction pairs
are byte-identical. Artifact12204707644 ZIP SHA256:
`62d4b0d8b22a7e7edf3172be2d8e5ecb368c79a24b5ffa5788cee7265d2a19c4`.

Independent review verified the exact nine ZIP entries/eight sizes/hashes,
source/run/tree binding and all200 immutable compiler fingerprint inputs:
`498c1f7930382384911e96d127b17698f6871ad2c18344e86fdb894f8e0f2dc6`.
Full catalog cells, core/dependency/ABI/compiler/target/flags fields match prior
manifests. Both Wasm phases have identical non-custom sections to the tested48de
tuple. Only the four exact actual generated outputs are adopted here.

[Raw source/hash/review receipts](m4-pr263-bounded-repair-bootstrap-2026-10-19.tar.gz),
archive SHA256 `5356ca2401fb6e6ce6e491eea639768f9402fd81c7024431be0adddfe23fe5e7`. The generation job failed the expected
stale-shipped comparison and did not execute later tests. Byte correspondence
does not validate the changed native compiler/test code. Fresh committed-head
reproduction, identity, bootstrap validation/execution and focused/library/full
gates remain required; default core17 adequacy is unproven.
