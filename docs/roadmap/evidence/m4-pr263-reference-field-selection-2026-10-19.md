# Native declaration-field selection across key origins

Source base: `422f750d16c66c630b107343df6a4806553ada26`.
Reviewed patch SHA-256: `0f4b76ee215bfe8bc5f7051a38d4fe5ac42018bbaba048330f4bee3e1fded1db`.

Global-reference projection looked up `:tag` and removed `:name`, `:op`
and `:ns` using cold keyword recipe identity. Literal and source key recipes
introduced by retained declaration provenance have different recipe IDs. A
completed literal tag could therefore be lost, and reference overlays could
leave duplicate semantic fields.

The native selector now recognizes actual unqualified keyword recipes across
literal, cold, source-form and alias constructors. It performs no guest hash or
equality call and grants no literal provenance from spelling. A cumulatively
charged iterative traversal detects missing/cyclic recipes using the existing
arena length. Reference overlays preserve an existing key and slot while
updating its value; an absent explicit overlay creates its own literal key.
The declaration is copied, and operation/name/namespace value construction
retains its prior order. Pinned upstream provenance is ClojureScript commit
`c4295f303100bbf5afac449242d30bca1126f1a1`, `analyzer.cljc` resolve-var
(lines1320–1445); the existing upstream license/provenance files apply.

Two native regressions cover mixed origins, tag selection, aliases, false
values, cycles, field retention and actual materialization/decoding after GC
in both phases. A compiled-macro regression checks actual AST/info tags and
resolved names, declaration metadata, GC and live/restored Keyword IHash in
both caller phases. These are added tests, not passing results. Array-map
paths are not required to manufacture hash callbacks.

Independent source review found a test-only invalid move from a Form with
Drop. The correction uses `into_parts().2`, retaining all assertions; follow-up
review verified that sole correction and returned GO SOURCE-ONLY. Four Rust
postimages pass rustfmt parsing, and git apply --check passes. These checks do
not establish typechecking or execution.

CI39259304741 for the base source is terminal failure: the foundation generated
bootstrap artifacts twice and then found shipped runtime.wasm differed. Its
library tests were skipped. Integration logs contain bootstrap identity
rejections; no Rust E-code compiler errors were found in inspected shard logs.
This does not classify every failing test as the same cause. Shipped bootstrap
files remain unchanged at the parent66a55ce versions and require normal
regeneration. No native test, issue acceptance or promotion is claimed here.
