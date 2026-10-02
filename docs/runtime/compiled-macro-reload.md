# Compiled source macro reload in progress

The compiled phase host now accepts macro libspec metadata
`^{:reload :reload}` and `^{:reload :reload-all}`. Plain reload reads the requested
macro namespace again while reusing successfully initialized dependencies.
Reload-all reads and initializes its reachable Macro-phase dependencies in
source require order. Unreachable namespaces remain loaded. Runtime dependency
cells stay separate; existing compiled Runtime expansions and captured functions
retain their original behavior while later expansions use the new macro roots.

Discovery validates immutable selected source snapshots before changing loaded
identities. Once initialization begins, selected phase identities are removed
from the provided set and are restored only after each source unit succeeds.
A failed macro definition preserves its previous owned function root; a failed
load is retried by the next ordinary import. Earlier executing Macro effects
remain isolated in that Store. Runtime compilation failure preserves caller
scope and publishes no Runtime input bindings. This follows existing session
exception/effect behavior, not transaction rollback of arbitrary language effects.

Reload policy is explicit compiler/host data, separate from source declaration
catalogs or Wasm binding cells. Nil/false reload metadata leaves normal once-only
loading in effect. Repeated metadata prefixes follow the pinned reader merge:
the outer prefix overrides inner metadata, and only the effective reload value
is validated. Malformed effective policy values fail at the libspec before Runtime
publication. The bounded bootstrap core cannot be source-reloaded. Ordinary
Runtime require reload metadata and whole-clause reload flags remain unfinished
and keep diagnostics; this change handles explicit source macro libspec policy.

Six native host regressions execute changed source, reachable helper changes,
retained Runtime expansions, failed definition/retry, unreachable module reuse
and metadata errors. One actual command regression executes `:load`, `:reload`
and `:reload-all` with source macro reload metadata, once-only initializers and
retained old Runtime functions. Existing import/namespace/phase tests remain.

This is source reload and initialized-module invalidation, not complete artifact
caching. Cache keys covering source/compiler/runtime ABI/macro graph/target/flags,
reproducible versioned bounded Java-free bootstrap, `scripts/verify-bootstrap.sh`,
full &form/&env metadata/maps, syntaxquote/splicing/gensyms, privacy/inferred
imports/shared AOT frontend and legacy evaluator removal remain required. #15
stackless cancellation, pending I/O cleanup/dynamic scope and actual live GC
accounting remain required. M3 is not complete.

Implementation is original Rust. Libspec reload metadata and policy lookup were
checked against pinned analyzer.cljc3484–3486/4490–4510 at
c4295f303100bbf5afac449242d30bca1126f1a1 (EPL1.0). No upstream code was copied;
structural defmacro adaptation retains its documented source/license provenance.

Prefix precedence was checked against pinned
`src/main/clojure/cljs/vendor/clojure/tools/reader.clj` 372–386: outer metadata
overrides the metadata of the recursively read inner form. This is an original
Rust lookup over retained reader syntax; no upstream reader code was copied.
