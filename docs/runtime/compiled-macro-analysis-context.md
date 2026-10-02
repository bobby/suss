# Compiler context and field facts

This is partial issue #14 work toward rich compiled macro `&env`. Source macros
still do not receive that environment. Rust expansion-host fixtures inspect actual
compiler facts and execute resulting artifacts in both Runtime and Macro Stores.

`AnalysisContext` distinguishes statement, expression and return contexts. It is
independent of recur legality: a return context does not grant a tail position or
an enclosing loop target. Operands and initializers use expression context; body
forms follow the pinned analyzer's return/statement rules. A bare `try` preserves
its caller context; handlers or cleanup introduce return context where appropriate.

Pinned `try` analysis expands finally, catches and then the body. That ordering is
observable through compile-phase macro effects, so Suss now matches it. Generated
runtime regions remain body, handler and cleanup; runtime evaluation, exception
selection and cleanup order are unchanged. A before-fix executing regression shows
both the previous traversal order and the incorrect bare-try context.

The development oracle force-compiles the pinned local ClojureScript source with
analysis caching disabled, then checks all 17 ordered context observations and 10
executed context result strings. Cached output without a new macro trace was
rejected during runner development. The native fixture compares the ordered
observations and independently decodes actual UTF-16 results after GC in both
Stores; it also verifies non-tail recur rejection and session recovery.

Field records retain real declaration syntax/metadata, source origin, field index,
mutability and lowered access expression. Inspecting a record does not read the
field. Parameters retain the actual field they shadow; lexical lookup still prefers
the parameter. Method scope restoration keeps the original field declaration and
access. Focused field execution and existing nominal/exception/hash regressions
exercise these facts and their unchanged runtime behavior.

```sh
CARGO_BUILD_JOBS=2 scripts/test-context-facts-oracle.sh
cargo test -p suss-cli --locked --test compiled_macro_analysis_context \
  --test compiled_macro_field_records --test compiled_macro_binding_records \
  --test portable_exceptions -- --test-threads=2
```

The oracle adds no shipped Java/JavaScript dependency. Compiler/fixture code is
original; no upstream core source or license counts change. Full child baseline,
independent PR review and final-head CI remain required. Genuine function scopes,
canonical rich environment transport, syntax quote/bootstrap/cache/evaluator
removal and the other original M3 acceptance work remain open.
