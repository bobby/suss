# Constructor source AST transport

Compiled macro environments retain the analyzed class and ordered argument
operands of `new` before native nominal construction. The source record exposes
`:op :new`, `:class`, `:args` and `:children [:class :args]`. Trailing-dot syntax
is rewritten to actual `(new Constructor ...)` syntax before source facts are
captured, in the existing analysis frame. Callee and arguments are analyzed once;
the physical construction path is unchanged.

Source type declarations record their field count before method analysis. These
genuine declaration facts supply the default `function` tag, `:type true`,
`:num-fields` and `:record false`, with source metadata retaining precedence.
Constructor result tags use the resolved global/local/field name, not runtime
descriptor inspection. A computed class has a present nil source tag; that is
not an inferred usable type.

The original implementation follows pinned `cljs/analyzer.cljc` at
`c4295f303100bbf5afac449242d30bca1126f1a1`: `parse-new`2670–2696,
`parse-type`3614–3649 and the trailing-dot rewrite4321. No core forms are copied.
The source inference adaptation retains its upstream copyright and EPL-1.0
notice and references the unchanged source hash; the distributed
[EPL license](../../runtime/core-import/epl-v10.html) is retained. Java and Node
are development-only oracles.

Nine fresh primary observations cover explicit, trailing-dot, local, zero-argument
and qualified constructors, current declaration revisions, preserved unrelated metadata and JS hint data.
Raw analyzer traces match actual Node observations,
including class declaration field presence, initialized fields and all eight
ordered effects. The strict checker has six positive/negative tests; all219 Python
checks pass. The native test compares the same frozen data in both caller phases
after forced GC and checks initialized fields and once-only effects. Compiler
regressions check shared source-analysis identity, original expansion order,
invocation-class nil tags, preserved current/revised metadata and JS hint precedence.
All five compiler regressions pass with no ignored or filtered tests. Both
bootstrap pairs have been regenerated. The native regression passes in both caller
phases with a finite 100M macro-operation test allowance (154.49s); the initial
default 10M run trapped. Production budgets and projection depth are unchanged. A separate native CLI
smoke observation reads operation/children and the initialized field at default
10M fuel; the corresponding two-phase, post-GC Rust regression also passes on the review
branch with no fuel override.

The parent compiled command returns `[nil nil]` for constructor operation/children
and exits1 on the explicit constructor-schema assertion. The first exploratory
oracle failed because `Box`/`Empty` collide with core types; distinct fixture names
repair it. A subsequent computed-constructor experiment failed at actual Node
execution and is not accepted as successful interoperability evidence. The
compiler-only unknown-tag regression states that limitation explicitly.

```sh
sh scripts/test-constructor-source-asts-oracle.sh
python3 -m unittest discover -s scripts -p 'test_*.py'
cargo test --workspace --locked --test portable_constructor_source_analysis --test compiled_macro_constructor_source_asts -- --test-threads=2
scripts/verify-bootstrap.sh
cargo test --workspace --locked -- --test-threads=2
```

Both fresh bootstrap phase pairs reproduce byte-for-byte with Java and Node
absent; identity checks and four executing bootstrap tests pass. On the frozen combined candidate, ten affected suites pass39 tests and the
full workspace baseline passes1315 with zero failures,41 tracked ignores and
zero filtered tests. The review branch is rebased directly on PR215 and adds a
default-budget regression. Exact review-base validation passes seven constructor
regressions with zero failures/ignores/filters, all219 Python checks, byte-for-byte
Java/Node-free bootstrap reproduction and four executing bootstrap tests. The
review-branch full workspace baseline and exact final-head CI remain required. These selected records do not prove the
complete portable analyzer schema or inference policy. Pending-I/O continuations,
cancellation and remaining original M3 acceptance obligations are unchanged.
