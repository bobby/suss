# Normal CI-generated bootstrap adoption

Run [39277766292](https://github.com/bobby/suss/actions/runs/39277766292), artifact
12197893480, generated both phases twice without Java/Node through the normal
suss-bootstrap CLI. The100M fixed-catalog analysis allowance sufficed for both
complete generator invocations. Production Rust compiled without E-code errors.
The verifier then failed its shipped runtime.wasm comparison and did not execute
its later identity/bootstrap tests. Optional diagnostic upload retained all eight
files, rather than converting that failure into a pass.

Source binding: branch head a05cbb115c57351f40c2c7fa934122d989c41b15 and CI merge
checkout0dcd60fdb23c16ac099818128aecfa4d3edf9ae3 have the exact same tree,
9e7ca3bd5a42bfc9064cb500eba44d68eacfc68b. The receipt reports a clean checkout;
GitHub artifact/commit APIs confirm the run/head/merge parent/tree. All nine ZIP
payloads match extracted bytes and every recorded hash/size was checked. Each
first file equals its second counterpart byte-for-byte.

| Adopted output | Bytes | SHA-256 |
| --- | ---: | --- |
| runtime.wasm |2108122|a470145c6624b8d3de588bfdbcb4084655a74d6f0f2c724846dd37c726c0477a|
| runtime.json |47413|c9d9b7bed693af0ab6fec78b8eb5ddf9dde82a0108386b1c1339c61f423e95b1|
| macro.wasm |2106643|cd085c5efea5a047e396e04638fc07b2b07e1b2c4061d697d6389eb6ca6434ca|
| macro.json |45875|06e0b0e57c7eb338d621718d4e696fdf971d3e4b0673493ffd837300669f58f4|

Both manifests bind compiler source aa7309f2681f51406f68b97f61e58c47ea6b5d2db057646b114eec6c778f5d3f,
core SOURCE51f4f48bbb9a3a75ed38a9167da90182fe787aad0a95d0fe587d3226bcdc190f,
and dependency graph c3de3bc234dd97169f42dd1ef790cb570375ceadf5bf61681ac6f7b48aab9325.
Phase, target, ABI2, empty flags and full Wasm hashes match. The actual compiled
build-script executables independently fingerprint current inputs to that compiler
identity. Independent review also recomputed it from200 immutable Git inputs and
approved adopting exactly the four outputs, pending next-head validation.

After adoption, scripts/verify_bootstrap_identity.py passes with the existing
compiled build script: checkout independence and all embedded-library/numeric/WIT
invalidation checks. It invokes no Cargo or Suss VM and does not establish runtime
behavior. The unchanged parent full-baseline producer remains separate and active.

Both images retain six noncustom section IDs1,2,3,7,9,10, but their noncustom bytes
DIFFER from the old parent images. This is not an identity-only refresh, and old
runtime test results do not validate the new images. Next-head normal reproduction,
artifact validation/linking and complete compiled_bootstrap execution are required,
followed by the original focused/pinned programs, full baseline and final-head CI.
No native execution/whole issue acceptance or promotion is claimed here.
