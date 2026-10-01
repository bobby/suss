# Quoted identifier and list values

Compiled macros need real language data rather than source strings or host syntax
placeholders. The portable pipeline now lowers bare keyword literals and quote
of nil, booleans, binary64 numbers, UTF16 strings, symbols, keywords and nested
lists to the shared GC runtime. Quoted code is data: its callee names and operands
are never resolved or executed. Identifier constructors use canonical core cells,
and nested lists use the retained core list implementation. No constructor fixture
or alternate macro interpreter is introduced.

The retained Symbol/Keyword types include every upstream method, their exact field
layouts and named/equality/hash/metadata/IFn/printing protocol declarations. Twelve
new source selections retain source hashes/EPL notices; fixed defn bootstrap and
js/Error construction have explicit patches. All reviews remain in-progress.
The imported profile now selects128 forms/132 licensed artifact files. Pending
get, str_ and writer dependencies remain uninitialized and error if invoked;
retaining their callers does not certify those operations. General symbol/keyword
constructor conversions, IFn lookup, unsupported-receiver name/namespace error
coercion, printing and complete metadata remain unfinished.

Literal hashes follow pinned compiler emits-symbol/emits-keyword: they are cached
at construction and independent of redefined public hash helpers. The original
Rust constructor lowering and EPL-1.0 constant hash port are in
crates/suss-compile/src/portable/hir/quotes.rs. Arithmetic mirrors retained
m3-hash-unencoded-chars/hash-symbol/hash-keyword over UTF16 with signed32 wrapping;
it does not use the prototype xxHash helpers or change the hashing algorithm.
Pinned source c4295f303100bbf5afac449242d30bca1126f1a1 provenance:

- core.cljs SHA256: d59e4703d36788b0ec562a778ec2e41188358aa964dee23a9b91e91fee26a80b; hash declarations1003–1017/1110–1113/3459–3460 are retained in the source import inventory and extractions.
- cljs/compiler.cljc SHA256: 06a4f301b7e27b55683b4a534667a1ee56ebe02758763a8359fd875dda4b25f4; emits-keyword/emits-symbol constructor paths368–399 inform original lowering; no compiler implementation is copied.
- Original upstream copyright and EPL-1.0 notice are retained in quotes.rs and extracted forms; runtime/core-import retains the upstream license text.

Executing evidence:

- scripts/test-quoted-identifier-oracle.sh records48 fresh pinned primary observations and compares lossless tags/UTF16/binary64 values. Private hash-symbol access warning remains visible.
- portable_quoted_identifiers.rs independently decodes Number/string/sentinels, descriptor-identified Symbol/Keyword/List/EmptyList and exact fields/counts after GC. Unknown nominal/array/sentinel layouts fail; no printer/equality call decides observation success.
- Three native tests cover nested quoted data/UTF16, all48 primary cases including cached/runtime-computed hashes, and located compilation failure without publication or effects. Returned identifiers survive loss of global roots and collection.
- Existing eight command/four atom/four namespace tests pass unchanged.

The pre-change regression fails on unresolved quote. An initial retained source
compile fails on js/Error, repaired through the typed private runtime adapter.
The first independent decoder fails because it assumes a class closure directly
owns its descriptor; actual closure property storage wraps the original environment.
The corrected decoder validates that wrapper and descriptor layout explicitly.
No unknown value is replaced by success.

Quoted vector/map/set values and runtime reader metadata still require actual
persistent collection types and are explicit located diagnostics. This work does
not execute defmacro or establish syntaxquote, gensyms, &form/&env, phase loading,
macro cache invalidation or reproducible compiled bootstrap. Those #14 requirements
remain active; the tree evaluator is not removed until compiled acceptance passes.
Full workspace, independent PR review/fixes and exact final-head CI gate readiness.
M3 and all four milestone issues remain open.
