# Native namespace loading and reload

The compiled native REPL supplies `:load name`, `:reload name`, `:reload-all name`
and `:in-ns name`. These commands require exactly one namespace name. Loads and
reloads return nil to the command; initializer results are not displayed or
reevaluated. Load/reload preserves the caller's namespace and aliases; `:in-ns`
changes scope without claiming that a file has been loaded. Source roots use the
existing Session options (the command default is `src`).

`Session::load_namespace` initializes a module once. `reload_namespace(name,
false)` reads/compiles/initializes that module again while reusing already-loaded
dependencies. `reload_namespace(name, true)` recompiles its reachable dependency
graph in require order. Unrelated modules are not reinitialized. Both reuse live
binding cells. Global reads see new definitions, while captured old function
values retain their behavior and GC roots. `defonce` continues to skip already
initialized bindings during an explicit reload.

Graph preparation snapshots all sources, compiles every module and validates/links
every artifact before initialization. A compilation/link failure preserves the
previous loaded identities and bindings. Once reload execution begins, selected
identities are marked unloaded until their initialization succeeds. Earlier
arbitrary effects and successful dependencies are retained. A failing definition
initializer preserves its old binding, and a subsequent ordinary load retries
failed/unexecuted modules without replaying successful dependencies. The caller's
namespace scope is restored by staged preparation before initialization.

The supplied `suss.core`/`cljs.core` profile cannot be replaced by file reload;
`:reset` reprovisions it. This is an explicit host profile policy, not a claim
that source `^:reload` libspec metadata is implemented. That metadata remains an
unsupported diagnostic. Source-level macro imports, complete namespace/private
Var behavior, compiled macro caches and full M3 acceptance remain unfinished.
There is no new source/artifact cache: explicit reload reads a fresh immutable
snapshot rather than accepting stale compiled bytes. Full cache keys and macro
invalidation remain #14 work.

`cargo test -p suss-cli namespace_session --locked -- --test-threads=2` executes
four regressions: actual command once-only loading/reload and captures; changed
source, compile/initializer failure and retry; reachable dependency order with a
reverse-lexical effect trace and unrelated module preservation; deterministic
missing/mismatched namespace diagnostics and successful dependency reuse after
failed reload-all. Independent native Number decoding observes exact binary64
values, and existing command/session suites remain enabled.

The original command regression fails before implementation with located
unsupported command/unresolved namespace diagnostics. Implementation/tests are
original; the existing namespace grammar and live-cell contract follow the pinned
ClojureScript source and accepted design. No upstream implementation is copied,
no ABI/dependency change is introduced, and no shipped JVM/Node path is added.
Full workspace, independent subagent PR review and exact final-head CI remain
required before publication readiness. Issues #13/#14 and M3 remain open.
