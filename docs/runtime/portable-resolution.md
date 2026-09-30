# Portable namespace and live-cell boundary

`portable::analyze_in` and `portable::compile_in` accept an explicit
`resolve::Environment` and `Phase`. They use the same lossless reader, HIR,
verified control-flow IR and shared-ABI emitter as `portable::compile`.
The latter supplies a default runtime `user` environment. Existing CLI/AOT,
component compilation, macro evaluation and the replaying REPL still use the
prototype. This is an integration boundary for their replacement, not a claim
that those clients have migrated.

## Resolution and source identity

Global identities contain phase, canonical namespace and symbol name. They do
not contain emitted type/global indices. `cljs.core` and `suss.core` resolve to
one canonical identity, including live cells. Each namespace and phase retains
its own aliases, explicit refers/renames and core exclusions when re-entered.
Lexical locals win over ordinary vars. Explicit refers and current-namespace
vars precede implicit core lookup; conflicting explicit bindings or namespace
aliases fail rather than depending on insertion order. Invalid configuration
fails before mutation. Analysis borrows the environment, so a compile failure
cannot install bindings or change aliases.

Runtime and macro-phase namespaces, scopes and cell identities are separate.
The API caller selects the phase and supplies declarations/imports. Tests execute
the same qualified source against different phase cells. This does **not** run
compiled macros, implement source `:require-macros` or create an isolated macro
Store. The future macro-session adapter must own that isolation.

The bounded bootstrap macro lookup is separate from ordinary var lookup: a
runtime var named `let` does not hide its core macro, but a lexical local does.
Qualified/aliased/referred core `let` works. True `if`/`do` special forms bypass
ordinary lookup. Arbitrary macro definitions and expansion remain unsupported.
Arithmetic bootstrap bindings still lower to checked intrinsic identities;
live callable cells and computed/local callees now execute through universal
closure calls; see [closure lowering](portable-closures.md). Bootstrap core
function values remain unsupported.
They never silently fall through to a core intrinsic. Dynamic cells are always
HIR/IR `Value`; they carry no inferred Number fact across redefinition.

`locate_source` searches all configured roots and `.sus`, `.cljs`, `.cljc`
extensions. It canonicalizes paths to deduplicate the same file, maps dotted
names and hyphens to directories/underscores, and rejects missing or ambiguous
sources with the supplied span. It uses the canonical core namespace. This is
a path resolver, not a file loader: reading/validating the declared source `ns`,
dependency cycles, privacy, source `ns`/require forms and module initialization
are still integration work. Namespace segments are currently alphanumeric,
hyphen or underscore; other forms fail explicitly.

## Executable cell imports

HIR retains the resolved cell identity, byte span and reader metadata. Ordered
IR `GlobalRead` instructions emit one `binding-get` call each. No backend source
re-evaluation, hoisting or type inspection can duplicate a read. Cells import
under `suss.bindings.runtime` or `suss.bindings.macro` with `namespace/name` fields.
Only used identities are imported, deduplicated and deterministically ordered.
Each import is an immutable Wasm global whose value is the **exact non-null shared
binding-cell type**, rather than a generic struct or eqref. The global points to
a mutable GC cell. Missing imports and incompatible objects fail at linking.
The runtime's checked bound flag distinguishes nil from an uninitialized cell:
`binding-unbound` constructs an unbound cell; `binding-get` raises a tagged
`Unbound binding` language exception; `binding-set` installs the value and marks
it bound. Arity and unbound errors have separate rooted descriptors. No recursive
ABI layout or public ABI identifier changed.

Old compiled code reads a cell's new value after `binding-set`. Previously
returned values remain rooted through later fragment compilation and forced GC.
This is not a persistent compiled session or source `def` implementation. A
production linker must share cell identity between fragments and validate the
manifest/prelude before linking; type compatibility cannot prove that two cells
represent the same language var. Dependency version/cache gates are future loader
work. Global reads are ordered and may throw; generic effect/exception analysis,
catch lowering and optimization remain incomplete.

## Evidence and next step

Ten focused tests validate/link/execute actual fragments, independently inspect
numeric bits, nil and UTF-16 exception payloads, and test phase isolation, alias/
refer/exclusion persistence, core alias deduplication, missing/wrong cell imports,
unknown names, dynamic type rejection and source ambiguity. Runtime-import traces
prove once-only source order and short circuiting. A fresh pinned ClojureScript/
Node run matches the expanded 42-case portable corpus, including qualified core
`let` and new closures; the existing 14 compiled reader cases and 13 pipeline tests also pass.
The original implementation and new cases copy no upstream code. Resolution was
checked against pinned `cljs/analyzer.cljc` (`resolve-var`/`get-expander*`) at
`c4295f303100bbf5afac449242d30bca1126f1a1`; imported upstream implementations still
require the provenance/EPL process.

Commands: `cargo test -p suss-compile --test portable_resolution --test portable_pipeline
--test runtime_abi --locked -- --test-threads=2` and
`scripts/test-portable-pipeline-oracle.sh`. Full baseline and review evidence are
recorded in the handoff/PR. The legacy source corpus remains 9 differential
passes, 7 exact failures, 0 skips; all 1,065 inventory declarations remain
unassessed. M2 remains incomplete. Next, connect source namespace loading/definitions, extended signatures, portable
coercions and unified
AOT/REPL/macro clients rather than maintaining a second production pipeline.

Source namespace directives and definitions now use this environment in
[definition preparation](portable-definitions.md). Required declarations must be
supplied; recursive file loading and source macro imports remain unfinished.


Source GlobalRead now checks binding-bound before binding-get and yields internal
undefined for an uninitialized, already resolved source variable. This does not
initialize its cell; defonce remains able to run. Unresolved names still fail
compilation, and direct internal ABI binding-get still raises its unbound error.
Fresh [forward-declaration evidence](forward-declarations.md) supersedes the old
source-read assumption while preserving the ABI cell distinction.
