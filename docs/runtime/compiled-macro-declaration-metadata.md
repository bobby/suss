# Compiled declaration metadata

The declaration projection distinguishes namespace declaration records from
resolved-var AST records. Namespace entries default to qualified `:name`; explicit `:name` metadata can
override that field, as in the pinned analyzer. They do not
receive a default `:ns`. Explicit symbol metadata for the selected portable
fields retains its presence and original data.

An initializer's provisional catalog entry carries symbol metadata and source
coordinates, while its namespace snapshot retains the previous completed entry.
An explicitly supplied `:meta` stays present in provisional entries. A completed
entry adds `:meta` from verified original reader data, omitting `:test`, and lets an
explicit definition docstring override the symbol's `:doc`. Truthy `:declared`
metadata with an initializer retains the provisional metadata policy. A source
`declare` without an initializer has completed reader metadata, including its
generated `:declared` flag, whose positions come only from the verified original
symbol in the declaration form.

Top-level declaration `:line` and `:column` describe the definition form. The
corresponding properties inside `:meta` describe the symbol token, including
reader end coordinates and the source file. Native tests independently assert
the loaded module's canonical path before normalizing that path to the pinned
development fixture. The canonical `suss.core` namespace (also addressed as `cljs.core`) uses the
pinned analyzer's `"cljs/core.cljs"` file marker in declaration and completed
metadata records. No source expression is evaluated to obtain metadata.

Completed scalar tags and direct source function return tags use retained
compiler inference facts. `:ret-tag` is present when a direct function's return
tag is known; unknown return information stays absent unless explicitly supplied
by metadata. Dynamic function declarations infer their returns independently of
dynamic var-reference tags; false type hints fall through to inference. Non-nil
`:top-fn` merge data overlays all 18 selected fields, followed by the computed
return tag. Function arity and
argument metadata retain the policies described in
[function declaration facts](compiled-macro-declaration-functions.md).

The executing regression captures all 18 selected fields of the unchanged
29-case pinned declaration corpus at the original source-analysis points. It
compares both namespace snapshots and live catalogs in Runtime and Macro caller
phases, after GC. Retrieval batches contain at most two observations to stay
within the unchanged per-form transport bounds. The observer uses a finite
100-million instruction budget for constructing its field vectors; default
production fuel is unchanged. Maps are sorted only for the primary helper's
documented map-order normalization. Collection kinds and field presence remain
exact.

The metadata policy adapts `parse-def` and `source-info` in the pinned
ClojureScript analyzer, under EPL-1.0; the source hash and adapted line ranges
are recorded beside the implementation. The development helper and native
comparison are original test code. No additional dependency or runtime image
change is required for this CLI projection.

An independent review regression covers raw name/provisional metadata, false
hints, dynamic functions, test omission, core file markers and top-fn merge
precedence in both native caller phases after GC. Its separate development-only
[primary probe](../../tests/oracle/declaration-metadata-review-probe.clj) executes
the pinned analyzer directly; the original 29-case corpus stays unchanged.

Terminal commands and results are recorded in the handoff. Independent review,
the full workspace baseline and exact final-head CI are required before the PR
is ready. This remains partial issue #14 progress: arbitrary declaration/AST
schema fields, self-local method records, source and macro dependency cache
invalidation, temporary evaluator retirement and original M3 lifecycle
acceptance remain open. No inventory declaration is promoted to complete.
