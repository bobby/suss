# Type declaration metadata values: bounded pinned reference

The [probe and complete receipts](m4-pr263-type-meta-value-reference-2026-10-19.tar.gz)
characterize eight actual pinned analyzer declarations observed through real macro
`&env`, with previous/current metadata object references. Archive SHA256:
`33c01e7b9946e4ef1d1d4aaf3de02ff8f46b89b1ac1c20a033a999ff323fca4f`. Source pin: ClojureScript
`c4295f303100bbf5afac449242d30bca1126f1a1`; upstream checkout clean.

Two fresh Node processes exited0 with identical stdout, SHA256
`b0212f0fb5416ebe6ca6b6ce04a27fe55a11515f0c2bf0a12af245d1a87e8036`.
All115 retained generated inputs were hash-checked before/after. This reuses
previously generated pinned JavaScript; it is not a fresh ClojureScript build or
native Suss acceptance. Independent review verified the source/results and
current input hashes, and caught stale inherited receipt boundaries. Those are
corrected in the archive; the probe and results are unchanged.

Fresh types have no `:meta`. Prior ordinary declaration metadata maps survive
with the same physical reference; explicit current `:meta` values replace them,
including false. Prior shorthand inner `:tag number` remains while outer tag
becomes function, retaining the original tag key object. Prior function
`:fn-var`/`:ret-tag` survive; current nil tag/false doc override earlier values.
Repeated types retain the earlier explicit metadata map.

These observations guide the remaining native completed-value/occurrence repair;
key-history transport alone does not satisfy them. They do not establish record
transitions, full callback counts/order, persistent cache authority or native
object identity across invocations. The actual live method and compiler/namespace
state are restored in `finally`. No production change is included here.
