# Native expression and file execution

Native `suss -e` and file execution now use the same portable compiler, shared
runtime and isolated compiled macro session as the native REPL. The previous
prototype compiler, component probing and GC result-decoding helpers are removed
from these two entry points. Component-target and AOT compilation paths still
require migration; the temporary macro evaluator is not retired by this change.

The runner parses the complete input and preserves its original forms and source
origin. It stages every runtime fragment before executing any input or dependency
initializer. A later runtime compile error therefore leaves existing runtime
bindings untouched. Successful fragments then execute in textual order in one
Store, without replaying initializers. Macro declarations execute in the isolated
phase while later source is prepared, so old compiled expansions retain their
meaning across macro redefinition. On a preparation error, a declaration journal restores staged macro function
bindings, catalog/export records and the caller phase namespace. Successfully
initialized macro dependencies remain loaded; arbitrary expansion-time global
assignment and reachable object effects are not rolled back. Resident generated
code may remain until reset. Ordinary initializer failures retain the existing session contract.

Only the final script result is displayed. Discarded lazy values are not forced
by printing. File macros receive the original filename and reader positions,
without source slicing, printing or rereading between forms.

`NativeDisplay` owns a reusable canonical-data bridge for one session. Native
REPL reset clears it only after both replacement phase sessions succeed. The
bridge retains actual class roots and uses existing validated data decoding,
including GC identity and traversal guards. Repeated printing after setup adds
no resident fragments, cells or owned handles. Embedders printing repeatedly
should retain a `NativeDisplay` and clear it after reset; the convenience
`display` function owns a temporary context for a single call.

Numbers use the runtime's binary64 formatter, with the existing readable spelling
for NaN and infinities. Strings preserve UTF-16, including lone surrogates and
control escapes. Canonical symbols, keywords, lists, vectors, maps and sets retain
their decoded contents and order. Display omits metadata; strict compiled macro
transport still validates and retains metadata. Pinned ClojureScript defaults
`*print-meta*` to false in `core.cljs` line135. The renderer is original Rust code,
not a port of the full upstream printing implementation.

Decoding retains the existing 4096-node and 1,048,576-unit guards, and rendered
output is bounded to 8 MiB. Unsupported data fails explicitly. Raw source arrays,
arbitrary nominal objects and non-data values inside collections are not given
fabricated string representations; full printing remains a separate requirement.

Executing evidence and commands are recorded in the handoff. Initial command
regressions failed on prototype `&env`, `meta` and global assignment. A first
migration exposed unsupported vector display; the unchanged vector assertions
now pass, including original file positions. Focused tests also cover numeric
and UTF-16 spelling, forced GC, stable display residency/roots, reset, metadata
omission versus strict macro transport, discarded lazy values, and a late compile
error preserving an old runtime binding. Independent review, required full
workspace baseline and exact final-head CI remain publication gates.

This is partial M3 progress. Complete source environments/inference, source/macro
cache keys and invalidation, evaluator retirement, remaining frontend migration
and lifecycle/cancellation acceptance remain open.
