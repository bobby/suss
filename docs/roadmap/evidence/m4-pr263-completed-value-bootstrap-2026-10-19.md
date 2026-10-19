# Normal bootstrap tuple after completed-value build repairs

Native source `f8776e75b4e10735605e2c41f45ba4fa7755cec1`, tree
`03f9672af6c01b05094e0ec853ebfc04cb905e5c`. CI39296185995 checkout
`c77b96045320fae5ede3f61664721d58c8f61a1c` has the same tree and the source
head as a parent. Foundation118209202388 builds the production Rust library and
runs both normal bootstrap generations to completion without Java or a JS
oracle. GitHub setup/upload actions themselves use Node; that is distinct from
the native generator.

Artifact12209405855 ZIP SHA256
`a35e1a98091f7bd6702975d12602a7d8c75040f6f9e9a450fb06c9d4172a4596`
has exactly nine safe entries: receipt plus four outputs from each generation.
All eight byte counts/hashes are verified, and the four pairs match exactly.
The original job stops at comparison against the old shipped runtime.wasm at
byte2107602. No bootstrap validation/linking/execution or library tests follow
that comparison failure; it is not a successful foundation gate.

## Source and output binding

Independent review recomputed all204 immutable Git fingerprint inputs and
matched the actual existing compiled build-script output:
`a6694768edbaf00f3acb42dafaedb67f6086ea2bf59ceb7b1a729d7be520c622`.
Catalog/dependency identities, ABI, phase, target/options and cells are unchanged.
Both Wasm modules' non-custom sections1,2,3,7,9,10 are byte-identical to the
previous320 shipped tuple. This byte comparison does not prove native restore
or new Rust test behavior. Only the four reviewed outputs are adopted.

| File | Bytes | SHA256 |
| --- | ---: | --- |
| runtime.wasm | 2108122 | `d608349219a60d40ba0cf99a3164c3e25237f46481cbf3ea0cbd16a93d31db40` |
| runtime.json | 47413 | `aea556566f4b0c011bc90288f62b3f94660470a27ac04d9e6d6d1d6607a144af` |
| macro.wasm | 2106643 | `ee17580f8a67bac85ec880133e55156c60cc9969353aac8fccffe7138dc3ab4f` |
| macro.json | 45875 | `2acab5b1f5615023e48a3db9c2f8fcfbb85429d52b5a8635d2c35b57fcdbe4b0` |

The local existing compiled build-script identity probe passes after adoption,
including checkout relocation and embedded dependency/WIT invalidation. It
executes no Cargo or Suss VM. The separate root66a55ce workspace producer is
untouched and remains the only local Cargo/VM producer.

[Raw generation log, complete source/hash receipts, independent review and
identity result](m4-pr263-completed-value-bootstrap-2026-10-19.tar.gz),
SHA256 `ff2bcb3f3433c50a5165ace0271d1c5f7ba53a6cfa6ae2e4567ab25f0356b390`. The archive contains a payload hash inventory; the four actual
outputs are committed directly under runtime/bootstrap rather than duplicated
in the archive. The [source build failures and marker correction](m4-pr263-type-value-build-repairs-2026-10-19.md)
remain qualified historical evidence.

## Required execution

Run normal Java/Node-free `sh scripts/verify-bootstrap.sh` on this new committed
source/bootstrap tuple, then focused completed type-value, retained-key,
analyzer-literal and COW ownership/fault tests. Replay unchanged source_roots,
core_import and both compiled_macro_source_tags programs, preserving strict
default controls and caller Runtime allowances. Default core17, the complete
one-scope source-child negative, actual public retained-reader/namespace
identity, new callback witnesses and added capture costs are unproved.
After focused acceptance, run the exact required workspace baseline and
independent final-head review/CI. No native or issue acceptance follows from
this artifact adoption alone.
