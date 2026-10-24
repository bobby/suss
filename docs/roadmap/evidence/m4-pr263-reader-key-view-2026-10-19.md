# Reader metadata key construction facts

Reviewed source patch SHA-256:
`ee701a612eee220874f8983147dcf5edcd9d98da31f06135f7e62c7936547973`.

The native metadata view now retains a private SourceSyntax/ReaderShorthandTag
key origin. Only symbol/string metadata desugaring constructs the latter.
A later association updates the value while preserving the earlier key and
its origin. Raw `:tag` spelling and synthesized values confer no provenance.
The owned adapter still projects the same Forms and discards this fact; no
Graph/FormBridge transport improvement is established by this view alone.

Independent source review checked exact constructors, first-key association,
complete invalid-prefix validation, unchanged owned normalization and charges
for the enlarged entries and added controls/sort movement. Added unit tests
cover both prefix orders, separate shorthand occurrences, keyword controls,
late invalid prefixes and every injected meter-call failure followed by retry.
Rustfmt parsing and cached patch application pass. No native typechecking,
bootstrap generation or test execution was performed for these additions.

Reference development probes using retained generated ClojureScript at pin
`c4295f303100bbf5afac449242d30bca1126f1a1` observe a numeric initial tag-key
memo for symbol/string shorthand, a nil initial memo for raw source tag keys,
and retention of the earlier key under either mixed-prefix order. They also
observe the same retained key in the outer declaration and inner metadata.
These observations define required transport behavior, not a native pass or
fresh reference compiler build. Unhashed array-map keys supply no callback
transcript. Existing upstream provenance and license notices apply.

Further wiring must share each genuine retained occurrence between the outer
declaration and inner metadata; independently normalizing twice or globally
interning all tag keys cannot prove that property. Source enrichment must
preserve this fact without inferring it from spelling or pointer/content
similarity. Normal bootstrap regeneration and actual both-phase transport,
GC, custom hash, restoration and fuel/interrupt tests remain necessary.
