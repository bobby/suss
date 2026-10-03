# Reader metadata in compiled macro input

Compiled source macros receive reader locations on implicit `&form` and user
argument data when the original syntax and byte spans still match an immutable
source snapshot. Locations use one-based UTF-16 columns, CRLF-aware lines and
exclusive end positions. Loaded source modules supply their canonical file path;
string input has no file field. Expanded syntax with reused spans retains its
explicit metadata and does not acquire invented source locations.

The reader adds indexing metadata through an opt-in data path before conditional
selection. Ordinary compiler reading still retains explicit metadata only.
List prefix defaults merge beneath explicit fields in each prefix; outer prefixes
retain the pinned precedence. Symbols used as tag metadata keep their own reader
metadata. The pinned slash special symbol has no automatic position metadata.
Metadata expressions remain syntax and are never evaluated during construction.

The actual pinned analyzer macroexpander produces nine observations. Native
source macros match eight exact projections after GC in both caller Stores:
nested positions, CRLF/UTF-16, explicit overrides, generated syntax absence,
conditional selection, chained prefixes, slash and tag-symbol metadata. A separate
executing namespace-loader regression requires the canonical loaded file path on
all four inspected objects. The primary file observation uses a named indexing
reader; its synthetic path and line layout are not claimed identical to the
native module fixture.

The original native before baseline was 1 passed and 3 failed. Initial repaired
execution passed 3 and failed the file test because its expected `/var` alias
differed from the loader's `/private/var` canonical path. The corrected four tests
passed. Two additional before regressions reproduced erased source snapshot
errors and lossy non-Unicode filenames; both now yield located diagnostics.
A stale five-case helper assertion failed the first expanded attempt and was
corrected to require all nine identities. Final affected validation passes 27
native tests across four suites, including all ten metadata tests, existing
source environments, source macros and macro metadata. The reader suite passes
30 tests, and all 108 Python tests pass. Failed logs remain in the handoff.

`SourceOrigin` caches original and located reader trees. Provenance matching checks
syntax, explicit metadata, spans and binary64 bits, including NaN. Reader failures,
non-Unicode paths and a missing located counterpart are errors. Current bounds
are a 1 MiB source snapshot and 1,048,576 provenance traversal nodes; the existing
FormBridge data limits remain. An executed bound-failure test verifies a located
compile error preserves old bindings and performs no initializer effects, then
successfully evaluates the next macro input. Assignment syntax returned from
metadata remains data and leaves the observed global unchanged in both Stores.

Indexing metadata and list read-meta precedence adapt the pinned vendored
`tools/reader.clj` at `c4295f303100bbf5afac449242d30bca1126f1a1`, lines
181–239, 308–328 and 372–406. SHA-256:
`614e857a4d92e22edbaf38e4e87c3f499e8703fb7f930c23e1df136535cc4597`.
Nicola Mometto, Rich Hickey and contributor copyright and EPL-1.0 notices remain
in the adaptation. The distribution includes [the EPL license](../../runtime/core-import/epl-v10.html).
Provenance matching and test fixtures are original repository code. JVM ClojureScript
is a development oracle; the shipped path remains compiled Suss without Java.

Full portable AST/declaration/function/method schema and inference, automatic
macro expansion metadata beyond this reader-data boundary, namespace policy,
syntax quote/gensyms, versioned reproducible Java-free bootstrap, cache invalidation,
obsolete evaluator removal and lifecycle/cancellation acceptance remain open.
Independent PR review, clean full baseline and final-head CI remain required;
these focused results complete neither an issue nor M3.
