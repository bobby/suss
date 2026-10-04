# Compiled export metadata and freestanding WIT shorthand

Portable source definitions accept `:export` reader metadata. This is a compiler
attribute, not a new runtime metadata or JavaScript interoperability promise.
Native file and namespace compilation may fill missing **freestanding** selected
WIT function mappings from marked Runtime vars. `^:export` uses the source var's
name; a string or unqualified symbol can name a different freestanding function.
Explicit `--export PATH=namespace/var` mappings take precedence. Their resolved
Runtime vars are excluded from shorthand inference, including when other
freestanding functions still need inferred mappings. Unsupported markers on
explicitly selected vars therefore do not prevent the selected mapping. Multiple marked
vars for one function fail before replacing an output artifact. Interface
functions always require explicit mappings, as required by design section 10.
False/nil markers do not select exports. Missing mappings retain the assembler's
existing diagnostic. Unsupported metadata types, invalid Unicode strings and
custom qualified symbols require diagnostics or explicit mappings. No Runtime
initializer executes during compilation; the component initializes on instantiation.

The pinned compiler distinguishes raw declaration metadata from normalized
`def` AST export names. Macro `&env` namespace records retain raw true/false/nil/
string values. A direct initializer's namespace snapshot does not yet include
its new definition; the current analyzer catalog contains its provisional raw
metadata. Completed snapshots include raw metadata both directly and under `:meta`.
Target selection normalizes true to the source var without changing those records.

Primary reference is ClojureScript commit
`c4295f303100bbf5afac449242d30bca1126f1a1`,
`src/main/clojure/cljs/analyzer.cljc`, `def` parsing and declaration publication.
The Rust implementation and development probe are original; no upstream forms
were copied. Existing ClojureScript source remains under its upstream EPL notice.

Focused evidence:

- Controlled parent-code command fails on rejected export metadata (0 passed,
  1 failed); fixed `compiled_export_metadata` executes all four tests successfully.
- Actual file and namespace artifacts preserve effects, typed calls, GC and two
  fresh Stores. Duplicate markers reject before replacing output; explicit mapping
  selects the requested implementation. Interface shorthand rejects and explicit
  interface mappings execute.
- Both compiled caller phases reproduce raw snapshot facts. Seven fresh pinned
  analyzer observations have exact assertions in `export-metadata-probe.clj`;
  `sh scripts/test-export-metadata-oracle.sh` checks the pinned checkout before running.
- Both bootstrap images/manifests were regenerated; Java-free reproduction checks
  exact bytes and current compiler identity, with four executing bootstrap tests.

The 28 existing affected tests pass across six modules: source preparation, file
and namespace commands, analysis graphs, function and metadata declarations.
All 118 Python checks pass. Independent review, unfiltered baseline and exact-head
CI are pending. This does not complete rich portable macro environments, full
selected WIT adapters, project/main/component-host migration, evaluator retirement
or the original M3 lifecycle/scheduler acceptance requirements.
