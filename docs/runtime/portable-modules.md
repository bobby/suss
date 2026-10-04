# Portable source module preparation

`portable::modules::prepare_modules(namespace, roots, environment, phase, provided)`
discovers and compiles an immutable source dependency graph. It returns a
dependency-first `ModulePlan` with validated Wasm, exact source snapshots, paths,
phase-qualified module identities, dependencies, staged declarations and cell
identities. It executes no initializer and never mutates the supplied Environment.
The native [session host](portable-session.md), command REPL and native source
AOT routes consume these plans. See [the persistence/namespace acceptance
audit](m3-session-acceptance.md) for current executing evidence. Historical
progress notes below preserve earlier implementation boundaries; complete macro
evaluator retirement and lifecycle work remain open.

## Discovery and compilation

Source paths use the existing namespace mapping, including hyphens to underscores,
and accept `.sus`, `.cljs` and `.cljc`. Discovery checks every configured root and
extension, deduplicates identical canonical paths, and rejects multiple distinct
sources. Each source must have a leading `ns` declaring the requested namespace.
`cljs.core` and `suss.core` share the canonical identity. Reader conditionals select
the first matching `:suss`/`:cljs` branch in source order.

Graph discovery and ordinary fragment preparation share the same namespace header
parser. Syntax errors and unsupported options are diagnosed before searching for
dependencies. Supported libspec aliases/refers/renames configure the ordinary
compiler Environment after dependencies have been compiled. Missing sources,
namespace mismatches, ambiguity, cycles and invalid source/body forms retain the
physical source path and byte span where available. Root lookup failures have no
source path and use an empty span rather than inventing a source location.

Dependencies are visited in require order, with duplicate canonical identities
visited once. The complete graph is read before compilation; each module is then
compiled from its retained text through the existing reader/HIR/verified IR/backend.
No second disk read can change the source being compiled. A later compilation
failure discards the entire staged plan before any host allocation/publication or
initializer effect. Cycles are explicit errors. A temporary prototype guard rejects
dependency nesting beyond 64 with a located diagnostic.

## Host initialization contract

`provided` is explicit host authority for successfully initialized modules, or for
the bounded bootstrap core whose declarations and intrinsics the host supplies.
Declaring a namespace does not make it loaded. A provided identity must have a
matching phase declaration; the compiler cannot prove that its host supplied the
correct cells or initialized them. Runtime and macro module identities are distinct.
This phase separation does not establish isolated compiled macro execution.

A persistent host must validate all artifacts and actual imports before running
any initializer, reuse shared cells by Global identity, allocate missing unbound
cells and instantiate every module before invoking `eval` in plan order. Mark an
identity provided only after its initializer succeeds. Preserve completed effects
and successful dependencies if a later initializer fails; do not mark that failed
module loaded. Retrying it reuses already successful dependencies. A failed def
initializer preserves its previous binding through the existing write-after-value
lowering. Runtime language exceptions must be caught distinctly from engine traps.

The integration test host follows this contract with one actual Wasmtime Store,
shared production runtime and independently inspected GC values. It is test code,
not a shipped session implementation. The compiler plan does not impose reset,
code-residency, interruption, source cache, reload or runtime recovery policy.

## Evidence and remaining work

Eleven focused tests validate and execute actual modules: source imports,
dependency/diamond order and once-only initialization, provided module reuse,
conditional dependencies, distinct phases, explicit bootstrap core aliases,
canonical roots and ambiguity, located mismatch/missing/cycle/depth diagnostics,
compile-error atomicity and immutable source snapshots. Corrupting a later artifact
rejects the graph before cell allocation or effects. A failing initializer is
verified as the exact shared language tag with independently inspected NotCallable
descriptor after GC; prior effects/binding survive and retry does not replay its
successful dependency. The diamond lists siblings in reverse lexical order and
asserts the actual write trace, so accidentally sorting dependencies cannot pass.
An ordinary namespace declaration cannot bypass loading.

The existing 42-case portable source corpus is independently compared with fresh
pinned ClojureScript/Node output, then executed as actual Suss artifacts. The
legacy differential baseline remains 9 passes / 7 exact failures / 0 skips. These
checks do not certify general module/core compatibility. All 1,065 inventory
declarations remain unassessed.

Implementation and fixtures are original. Namespace/definition semantics are
informed by `cljs/analyzer.cljc` and `cljs/core.cljc` at pinned
`c4295f303100bbf5afac449242d30bca1126f1a1`, licensed EPL-1.0; no upstream
implementation is copied. Upstream ports still require source/patch hashes and
retained notices. No dependency or shipped JVM/Node path is added.

Production persistent clients, reload/cache/privacy policy, source macro imports,
compiled macro sessions, runtime metadata and broader language/core lowering remain
unfinished. Source reload metadata and `:require-macros` remain explicit unsupported
diagnostics. M2/M3 acceptance is incomplete. Next connect these staged plans to a
production persistent session with runtime recovery and no source replay.

Compiler `prepare_input` now prepares optional inline ns dependencies and the input
as one transaction, sharing discovery/header/compilation code with file plans.
Dependency failures retain file locations or inline input require spans. The
[native session host](portable-session.md) validates/links all artifacts before
initialization and publishes only successfully initialized module identities.
