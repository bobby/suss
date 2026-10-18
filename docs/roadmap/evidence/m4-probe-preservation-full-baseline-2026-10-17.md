# Probe-preservation integration: terminal local workspace baseline

Exact source head: `8a110949f1c558ade6c38999e61aeeaffade6c2d`.
This is the required local command on the independently reviewed integration,
not a fresh semantic fix or passing baseline:

```sh
cargo test --workspace --locked -- --test-threads=2
```

The focused gates preceded this run. Local baseline terminal exit101 after
3653.230284088 seconds. Jobs1/Rayon1/test-threads2; inherited RUSTFLAGS was not
assigned. Default test fixtures/allowances/fees were unchanged. All4,037 frozen
tracked/submodule/component inputs remained byte-identical, the recorded actual
head remained8a, and terminal Git status was clean.

## Actual execution and two failures

**88 Rust PASS, 2 FAIL, 0 ignored.** All18 started harnesses reached terminal
summaries: three CLI lib/bin harnesses (27passes) and15 integration targets
(61passes/2failures). The first and bootstrap CLI unit/bin harnesses contain zero
tests; those are executed empty harnesses, not additional passes.

The terminal integration target is `compiled_macro_collection_source_asts`:

- `collection_source_nodes_retain_original_children_before_factory_lowering`:
  guest Wasm fuel exhaustion **during compiled macro invocation**, span851..882,
  source assertion at `compiled_macro_collection_source_asts.rs:231:56`.
- `unordered_literal_ast_edges_and_runtime_effects_preserve_textual_order`:
  guest Wasm fuel exhaustion **during caller-&env input transport**, span510..531,
  at `compiled_macro_collection_source_asts.rs:291:18`.
- `empty_do_source_return_retains_context_and_nil_after_gc`: **PASS** in the same
  target. Do not classify the whole target as unexecuted or all-fuel.

Both failed tests stop before their required later value/effect assertions.
These same two names and fuel stages are present in the separately retained
exact8a CI and earlier f156 CI. This does not establish equal costs, a passing
whole suite, or that later unexecuted targets have no regressions.

## Target coverage and remaining execution

The captured locked Cargo workspace target manifest contains259 integration
targets. **15/259 started and finished;244 integration targets are UNEXECUTED**.
There are no unfinished started harnesses. Cargo's standard fail-fast exits
before later workspace integration targets, other workspace library/unit test
harnesses and doctests. Crate compilation does not mean their tests executed.
Raw individual harness names, full actual test results, and all planned/missing
package-target pairs are in `independent-terminal-review.json` and the original
`workspace-targets.json`. No ignored result or missing observation is promoted
to success. No Python tests were run by this Cargo command.

Keep this local baseline separate from CI39080390999 at the same source:
CI foundation536Rust passes/1ignored and353Python passes; integration111passes/
14fuel failures,30/259 targets terminal and229unexecuted. Those counts overlap
this run and are not additive distinct coverage. The independently classified
CI remains terminal red; its full logs and report are separate published evidence.

## Source correspondence and artifact

Before/after freezes are identical at4,037paths. `git-input-bindings.json` resolves
every frozen SHA256 to exact8a Git blobs or pinned CLJS
`c4295f303100bbf5afac449242d30bca1126f1a1`, including all four shipped bootstrap
artifacts. Each retained original CAS object's size, SHA256 and Git blob identity
was checked. The original CAS stays in cache; it is not duplicated into this
lean evidence archive. External Git dependencies are required, so this is not a
standalone full source distribution.

The archive retains the complete raw workspace log, command/terminal receipt,
both freezes, target manifest, original driver, exact committed-head review,
independent terminal classification and Git bindings. It omits only the bulky
original frozen source/CAS directory. No source worktree or index was modified
by the audit/packager. Artifact SHA256/member counts are in adjacent
`package-result.json`; the final repository handoff should cite them when root
publishes the unchanged package.

This establishes an actual terminal **failing** required baseline. It establishes
no passing full baseline, green CI, issue completion, promotion or merge.
Further source changes require their own focused review/execution; the captured
8a outcome must remain retained even if those later changes succeed.
