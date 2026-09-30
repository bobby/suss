# UTF-16 indexed storage for source sequences

The pinned IndexedSeq source uses alength and aget for array and string owners.
These operations now accept the shared UTF-16 string layout as well as the
existing GC-owned mutable array owner. Length counts code units. An in-range
numeric indexed read produces a new one-unit string; astral characters therefore
yield their separate surrogate units. NUL and lone surrogates remain exact.
Missing integer, negative, fractional, NaN and infinite numeric keys yield
internal undefined; signed zero indexes the first unit.

Macro and first-class calls use the existing checked HIR/IR and universal
invocation path. Nested indexing preserves ordered intermediate reads and
exception recovery. No helper/global/type index, recursive layout or ABI version
is added. Array storage, growth, clone and ownership behavior is unchanged.

The source reference remains ClojureScript c4295f303100bbf5afac449242d30bca1126f1a1:
core.cljs aget538/alength553, core.cljc aget1043/alength2748, and IndexedSeq1653.
Four existing hash-bound reviews record this original Rust adaptation; no new
upstream form is copied. Generated core artifacts retain their source and EPL
packaging with an updated review hash. These reviews remain in progress.

The separate 31-case corpus uses fresh compiled/executed primary observations and
independent native decoding of binary64, exact Boolean sentinels and UTF-16
arrays. Native GC runs between cases, including retained string owners and old
first-class aget/alength functions after public runtime redefinition. Explicit
qualified macros still use primitive storage behavior. Before implementation,
the native regression failed with a language error on astral string length.
Independent review adds large unsigned index boundaries and nested read/throw/
finally effects. A separate native guard verifies unsupported properties, writes,
clones and wrong runtime arities are language errors, followed by successful
lone-surrogate reads after GC; it does not claim compatibility for those domains.

```sh
CARGO_BUILD_JOBS=2 sh scripts/test-indexed-string-oracle.sh
```

This is a prerequisite for source IndexedSeq, not persistent sequence acceptance.
String writes, host named/coerced property keys, checked-array compiler modes,
compiled upstream macros and full source sequence/core integration remain
unfinished. Unsupported property inputs follow the typed language error path,
not Wasm casts or out-of-bounds traps. Complete UTF-16 slicing/hashing/printing
and persistent collection protocols still require their own acceptance evidence.
