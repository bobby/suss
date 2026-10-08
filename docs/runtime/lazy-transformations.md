# Full portable lazy map and filter source

This draft retains complete pinned ClojureScript `map` and `filter` declarations
and their whole `ChunkBuffer` dependency. It is original M4 work for #16–#19;
native tests now execute the full retained declarations; final publication gates
and the original M4 acceptance criteria remain open.

The upstream pin is `c4295f303100bbf5afac449242d30bca1126f1a1`. Originals,
notices, source hashes, ranges, patches and adapted hashes are reproduced by
`docs/compatibility/core-import.json` and `runtime/core-import/manifest.json`.
All existing main selections stay unchanged and ordered. EPL-1.0 notices and
upstream license files are included with the extracted library.

| Declaration | Pinned lines | Retained contract |
| --- | --- | --- |
| ChunkBuffer | 3673–3685 | Both mutable fields, Object add/chunk and ICounted; write before end increment, storage captured before clearing |
| chunk-buffer | 3687–3688 | Capacity forwarded to the existing owned-array construction adapter |
| chunk-append | 3800–3801 | Live nominal add dispatch |
| chunk | 3803–3804 | Live nominal chunk dispatch |
| map | 4902–4947 | Transducer and all lazy/multiple-input arities, complete chunked/nonchunked branches and named variadic step recursion |
| filter | 5370–5397 | Transducer and lazy arities, chunked/nonchunked branches, second indexed read after the predicate |

`ChunkBuffer` is extracted unchanged. Five hash-bound patches expand `defn` to
`def`/`fn`, preserving documentation and every signature/body. `lazy-seq` becomes
the owned pinned `LazySeq` constructor with a deferred `fn*`. `when-let` uses a
once-only temporary and conditional inner binding. `dotimes` retains its pinned
once-only bound, loop, ordered body and qualified comparison/increment expansion.
Map's single shorthand `#(apply f %)` becomes an explicit one-parameter `fn*`
that captures `f` and retains live `apply`. These explicit adaptations establish
neither general reader shorthand support nor full upstream macro compilation.
No collection branch or transducer reducer arity is omitted.

## Shared executing reference cases

`tests/oracle/lazy-transformation-cases.json` contains 41 closed, typed cases.
The same fixture and source expressions feed fresh pinned ClojureScript and
native tests. Forced compilation with analysis caching disabled produced 41
exact binary64/Boolean/nil/vector observations. Initial incorrect expectations
and failing observations are retained in the handoff's evidence paths.

Cases distinguish small vector `IndexedSeq` demand at 31/32 from chunk demand at
33/64/65. They cover empty/nonchunked inputs, nil/false values, shortest inputs,
all map input counts and transducer reducer arities, empty/rejected filter
chunks, truthy zero, arbitrary thrown false/nil with retry, Reduced identity and
callback/reducer effect ordering. Source realization order is recorded for two,
three and four inputs.

Live `first`, recursive map/filter, chunk and variadic apply replacements expose
lookup at demand. The first helper is invoked in the transformation body and in
`LazySeq.-first`, producing 21 from the original 1 with two additions of 10.
Multi-arity replacements retain the entrypoint shape required by pinned compiled
ClojureScript. A single-arity replacement initially failed with TypeError and is
retained as failed oracle evidence.

The backing-mutation cases distinguish map's value 1 from filter's value 99:
filter deliberately rereads its chunk after the predicate changes storage.
Finalization observations check backing identity, cleared buffer storage,
unchanged count after failed append and the unreadable repeated-finalization
chunk. These checks preserve the upstream behavior rather than supplying a new
post-finalization policy.

## Native gates and current limits

`crates/suss-cli/tests/portable_lazy_transformations.rs` authors four native tests
for both Runtime and Macro phases:

- Decode all 41 cases through bounded `FormBridge` after GC, with closed IDs and
  lossless tags; unknown result kinds fail.
- Retain captured callbacks across GC before demand and between independent
  source fragments, then verify cached-head reuse.
- Keep unforced map/filter results only in host handles, force GC and invoke a
  separately compiled projection.
- Inspect actual Wasmtime references to prove an ArrayChunk retains the same
  backing array after GC, independently of guest identity or printing.

All four native tests passed in 14.63s with zero failures/ignores/filters after
bootstrap regeneration; each test executes both phases. Their finite 100M
operation allowance follows the existing sequence stress policy but remains
unmeasured, so no default-budget claim is made. Full provenance,
bootstrap reproducibility, unfiltered workspace baseline, independent final-head
review and final-head CI remain required for PR readiness.

Twenty-four original complete-source CLI probes exited successfully and their
printed closed values matched. All 17 additional complete-source probes also passed with matching printed values.
They use unchanged production compiler source and provide supplemental execution
information; they do not replace the regenerated-bootstrap/two-phase/raw-storage
gates. A whole-library reload through the older REPL binary failed captured
canonical descriptor identities; exit zero with printed errors was recorded as
failure. Sorted collections, records and the remaining original M4 contract are
still separate work.

```sh
sh scripts/test-lazy-transformation-oracle.sh
python3 -m unittest discover -s scripts -p 'test_lazy_transformation_oracle.py'
cargo test -p suss-cli --test portable_lazy_transformations --locked -- --test-threads=2
scripts/verify-core-import.sh
scripts/verify-bootstrap.sh
cargo test --workspace --locked -- --test-threads=2
```
