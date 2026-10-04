# Persistence and namespace acceptance evidence

This audit maps issues [#12](https://github.com/bobby/suss/issues/12) and
[#13](https://github.com/bobby/suss/issues/13) to the accepted design sections3,
6–7 and actual executed regressions. It uses merged main
`1f724f02fccfd2e18506b1c754d34ffece5ac886`, whose tree is byte-identical to the
independently reviewed PR185 head `fdcfa45af13cade8a56ddd09da4e0b7dd41c2e4d`.
The complete final-head baseline passed 1187 tests in140 groups, with0 failures,
17 existing ignored and0 filtered, through all four doctest groups. Exact-head
[CI37207313040](https://github.com/bobby/suss/actions/runs/37207313040) completed
successfully on that reviewed head. The tests below were individually observed
passing in that full log, rather than inferred from its aggregate count.

## Issue #12: incremental compiled REPL

| Required behavior | Direct executing evidence |
| --- | --- |
| Atoms persist | `portable_atoms::persistent_session_atoms_and_old_contents_remain_rooted_across_fragments_and_gc` mutates and reads original atom captures after global rebinding and GC. `persistent_repl::persistent_session_command_repl_keeps_atoms_and_their_captured_values` runs the shipped command. |
| Closures persist and old values remain usable | `persistent_session::persistent_session_old_closures_and_owned_values_survive_rebinding_and_gc` invokes the old owned closure and inspects original UTF-16 after later inputs/rebinding/GC. Named-function and nominal regressions additionally execute old captures/descriptors. |
| Initializers do not replay | `persistent_session_initializers_execute_once_and_defonce_skips_effects` observes effects across later inputs; `persistent_session_nil_and_false_are_initialized_defonce_bindings` preserves false/nil bindings. Input dependency and failed-module retry regressions verify successful initializers are reused. |
| Fragments import the stable ABI | Session installation verifies runtime/source identity and links actual fragments to one Store/runtime and shared phase-qualified binding cells. Old closure/object execution crosses independently compiled fragments; the private `session_lifecycle_all_artifacts_are_gated_before_allocating_or_publishing_cells` rejects an invalid artifact and an actual missing import before publication. |
| Production compiled prompt | `persistent_repl` runs process-level persistence, recovery, reader and display regressions. The command evaluates each complete input once and displays the rooted result. Former source replay modules are test-only. |

The architecture has one long-lived Store per Runtime session, resident compiled
fragments and owned roots. Macro evaluation uses a separate Store. Those Stores
are intentionally separate phases, not a replay of the Runtime session.

## Issue #13: namespace loading and redefinition

| Required behavior | Direct executing evidence |
| --- | --- |
| Globals see redefinitions; captured function values stay stable | `namespace_session_command_loads_once_and_keeps_live_globals_and_old_captures` exercises the command; `namespace_session_reload_reads_changed_source_and_preserves_bindings_on_failure` checks live and old calls after changed source and GC. |
| Compile failure preserves existing bindings | The changed-source reload regression compares session statistics and original bindings and verifies no initializer effects occurred. Input dependency compilation is atomic before installation. |
| Failed initializer preserves existing bindings | The same reload regression retains the old value and live function, preserves completed earlier effects and retries the failed source. This is not rollback of arbitrary effects. |
| Namespace errors are deterministic | `namespace_session_errors_are_deterministic_and_reload_failure_keeps_dependencies_provided` compares repeated missing/wrong namespace errors and session state. |
| Load-once, defonce and reload policy | `namespace_session_reload_all_visits_only_reachable_dependencies_in_require_order` checks effect order, defonce, reachable reload-all traversal and preservation of unrelated modules. Macro reload tests check reachable/unreachable phase dependencies, metadata precedence and failed-reload retry. |

The referenced namespace contract also has executed coverage:

- `.sus`, `.cljs`, `.cljc`, canonical roots, ambiguous sources and located
  dependency errors: `portable_modules` executes real linked modules; the
  namespace command tests compile and instantiate actual components from
  `.cljs`/`.cljc` dependencies and reject ambiguous/wrong/missing sources.
- Canonical `suss.core`/`cljs.core` aliases, source-order conditional features,
  ordinary refers and renames: `portable_modules` and compiled command macro
  regressions execute the resolved bindings.
- Macro aliases/refers/renames and ordinary imports within the Macro phase:
  `compiled_macro_imports` executes the shipped prompt and checks exact outputs.
  Runtime and Macro aliases with the same spelling refer to distinct cells;
  local bindings shadow macro names.
- Exclusions and qualified symbols: command macro tests execute core alias
  access in an excluded namespace; `portable_apply_sequences` executes a private
  definition through qualified access and independently checks the pinned corpus.
  Privacy metadata does not turn ClojureScript warning policy into JVM access
  rejection.
- Namespace environment maps: `compiled_macro_analysis_graph` compares actual
  requires/uses/renames and corresponding macro maps with pinned observations
  in both caller phases. This is supplemental evidence for resolution, not a
  replacement for executing calls.

`portable_phase_modules` discovery assertions are additional phase-graph checks;
actual isolated phase execution is established by `compiled_phase_session` and
`compiled_macro_imports`, not by discovery or encoding alone.

## Remaining public legacy compiler path

The public `suss_compile::Compiler::compile_with_namespaces` still discovers
sources through `DependencyResolver` and calls `expand::expand_all` before the
prototype backend. Its `Compiler::ns_to_path` searches only `.sus` files and
returns the first existing candidate across roots. This does not implement the
design section3 contract to accept `.cljs`/`.cljc` and reject ambiguous namespace
sources. Native command success does not prove that public API's behavior.
The public legacy expression/file/project/main methods likewise call the
production tree-walking macro expander. The optional wasm component CLI also
imports a WIT evaluator rather than the native persistent session.

The scoped Rust caller search also finds legacy Compiler usage in the compile
crate's expression/component/conformance/oracle/toolchain tests, its performance
benchmark and `examples/test_compile.rs`, and the CLI's test-only session fixture.
Migration must preserve those semantic and failure observations; changing only
the command router or hiding these tests cannot prove retirement.

These paths require migration/retirement and executing acceptance at their actual
boundaries. Do not close #13 for the complete referenced public contract solely
from the native namespace command results. The listed #12 persistence criteria
have direct native executing evidence; coverage of any shipped alternate REPL
path must be resolved rather than assumed. #14's evaluator retirement remains
required; keeping the old resolver in production is not cured by documentation.
No alternate target build or runtime result is claimed from this source inspection.

## Executed cross-frontend coverage

A new `compiled_namespace_acceptance` regression uses identical source files in
the shipped REPL and native namespace AOT command, then executes the actual
component. It combines ordinary aliases/refers/renames, macro aliases/refers/
renames, an ordinary import inside the Macro phase, core exclusions, a canonical
core alias and two matching conditional reader features. The Runtime and Macro
aliases deliberately share the spelling `d`; the expected numeric result
requires each resolution to use its correct phase and the excluded arithmetic
name to call the user function. The candidate executed successfully: one regression passed in10.86 seconds,
zero failed/ignored/filtered. Both shipped prompt outputs and actual AOT component
calls matched53/57, including a call after GC. This supplies direct combination
coverage across frontends; it does not certify the public legacy compiler path.

## Review gate and remaining milestone work

The explicitly listed criteria have merged native execution evidence. The public
legacy namespace path remains an identified gap in the referenced contract. This acceptance
record still needs independent PR review, required full validation on its final
head and exact final-head CI before marking this partial acceptance PR ready. Issues
remain open during preparation. Their original proposed commands are:

```sh
cargo test -p suss-cli persistent_session
cargo test -p suss-cli namespace_session
```

Fresh focused commands on this acceptance tree also terminated successfully:
`persistent_session` has29 passing tests (494 filtered); `namespace_session` has19 passing tests (504 filtered). Neither focused count is an
unfiltered baseline claim. The combined new REPL/AOT regression passes separately.

The merged unfiltered baseline executes the matching tests plus ABI/atom/module/
phase/AOT checks whose names do not match those filters. Run focused acceptance
first on the final candidate, then the required unfiltered baseline:

```sh
cargo test --workspace --locked -- --test-threads=2
```

This record does not certify complete core compatibility, complete source macro
AST schemas, production macro evaluator retirement, cache target/flags policy,
cooperative pending-I/O cancellation, rooted continuations or live heap/leak
accounting. Those remain original #14/#15 and broader compatibility work.
Published project dependency support is a separate unresolved policy; explicit
rejection of unsupported nonempty project dependencies is not proof of support.
No M3 completion or PR merge is claimed.

The combined regression was re-executed after stacking on independently reviewed
PR #187 (`c0665d9`): one test passed, zero failures/ignored/filtered in 10.39
seconds. No compiler, runtime or bootstrap inputs changed in this acceptance PR.

## Independent PR #189 review

Independent review at candidate `d48af03` checked the new executing regression,
issue #12/#13 criteria, design sections 3 and 7, and the remaining public resolver
and evaluator call sites. No significant scoped test or implementation finding
was identified. Corrected the resolver method attribution to `Compiler::ns_to_path`;
the unresolved public-contract gap remains explicit. The focused regression passed
one test, with zero failed/ignored/filtered, in 10.40 seconds. Review log:
`/private/tmp/suss-pr189-review-focused.log`. Complete final-head baseline and CI
remain required; this focused result does not establish them.
