# Normal bootstrap after the type-test source dereference

Source `19f62117ff686a7b11b59710fb69721bb9c0c303`, tree
`a8385a71994c2cf2cd031ee158a5d60251e5b13b`. CI39298297611 checkout
`024e7f8c78f0b6d2cd2fa8b0cd8e6fe994ce1763` has the same tree and the source
head as a parent. Foundation118216930638 completes two normal generations of
both phases, without Java or a JavaScript oracle in the native generator.
GitHub setup/upload actions themselves use Node.

Artifact12209851821 ZIP SHA256
`2a9e58c68038b96fa6b324143a8fcaca9f703c6828b2442c25752cd173e4fa66`
contains exactly nine safe files. All eight output sizes/hashes and four equal
pairs are verified. Independent review recomputes all204 immutable Git inputs
and matches the actual existing compiled build-script fingerprint:
`d7bdce3f2208c5102f3173e0b0ce091437957cf3d60cb5f6ef98e3c476e0274b`.
All other catalog/dependency/ABI/phase/target/options/cell fields are unchanged.
Both Wasm modules' non-custom sections1,2,3,7,9,10 equal the previous3c tuple.
Only the four reviewed outputs are adopted:

| File | Bytes | SHA256 |
| --- | ---: | --- |
| runtime.wasm | 2108122 | `9e80ecd452d4ae694e2064e7d0df22b8f6a01d55405f3f3414cc7a7b9aeae2bb` |
| runtime.json | 47413 | `3cd92d806b449082a01e027d8aba4bef0342137049a6cfe82953779da606d4e6` |
| macro.wasm | 2106643 | `e76d6dab9af08a9e9d5b9bc4a3c39fc89ca27ffbfdff36dca515a14bc7925e74` |
| macro.json | 45875 | `3759edf3dc41bc9d7ab6e9cd8ad49e30bc97d1b83e416c199df221a899b4ddae` |

The source delta from3c is exactly the cfg(test) dereference; production Rust is
unchanged. The normal generator still stops at old-shipped comparison at
byte2107602 before bootstrap/library execution, and its later incomplete or
cancelled CI jobs are not successful. The actual local compiled build-script
identity/relocation/dependency-invalidation probe passes after adoption; no
local Cargo or Suss VM runs.

[Raw generation log, source/hash receipts, independent review and identity probe](m4-pr263-test-source-bootstrap-2026-10-19.tar.gz),
SHA256 `4c0565be0de86d6502e24672fc0b89c084c6dfd70cd5a0ba43114f04242c77cd`. Actual assets are committed directly under runtime/bootstrap.
The [earlier3c bootstrap4 pass and later failed test build](m4-pr263-type-value-test-build-2026-10-19.md)
retain their original source scope. Fresh normal reproduction, identity,
validation/linking and bootstrap execution are still required for this tuple,
then focused type-value/literal/COW/source_roots/core_import/source-tags
execution and the exact workspace baseline. Strict default controls, caller
Runtime allowances and accounting rates are unchanged. Root66a55ce's separate
workspace producer remains untouched; no current public identity, fuel adequacy
or complete issue acceptance is inferred from adoption.
