# Declaration tags in compiled source environments

Source macros inspecting analyzed variable references and invocations previously
received declaration hints instead of the actual pinned analyzer's completed
record policy. False scalar hints suppressed computed tags, dynamic functions
acquired a scalar `any` tag, and present nil function tags disappeared. Invokes
also inferred from provisional function bodies and missed raw return metadata.

Compiler source facts now separate raw tag presence from inferred type values.
Completed scalar records use a truthy hint, dynamic fallback or actual initializer
inference before retaining otherwise unmodified raw metadata. Function references
retain their raw/top-fn-overlaid tag, including present false and nil, independently
of return inference. Provisional records expose raw metadata. Invocation inference
requires the published fn-var policy and uses completed return information or raw
return metadata. Local and parameter false hints stay intact. A nil AST tag remains
present without becoming a non-nil inferred type. Execution and storage are unchanged.

## Provenance and executing evidence

Policy follows pinned ClojureScript
`c4295f303100bbf5afac449242d30bca1126f1a1`, analyzer.cljc1529–1658,
2092–2175 and2484–2529, SHA-256 `297802c627474434f1ef868e31f5f9913c290a4e80c509a40c704dced95bbf47`.
This is original Rust analysis code. The development source retains upstream EPL-1.0
license/notices in the pinned submodule; no Java dependency is added to shipped code.

The original `tests/oracle/source-hint-review-probe.clj` executes the actual pinned
analyzer and writes `source-hint-review-observations.json` (schema1, pinned revision,
18 rows). Run from `tests/oracle` with
`CLJ_CONFIG=/tmp/suss-oracle-clojure-config CLJ_CACHE=/tmp/suss-oracle-clojure-cache clojure -Srepro -M source-hint-review-probe.clj`.
It preserves presence and value kinds for selected `:tag` and `:inferred-ret-tag`.
No expected result was inferred from static inspection or substituted for unknowns.

The native regression executes all 18 expressions and inspects actual compiler
records through a compiled macro, then independently decodes rooted data after GC
in both caller phases. Fifteen scalar/function/effect assertions per phase verify
runtime storage and once-only initializer effects. Before the fix nine rows per
phase failed; after the fix all18 rows and all runtime assertions pass. Helper and
caller production fuel/graph bounds are unchanged.

Affected suites, Java-free bootstrap reproduction, independent review, required
exact final-head full workspace baseline and CI gate PR readiness. Current results
are recorded in the handoff. Full environments/inference, source/macro cache
invalidation, evaluator retirement and pending-I/O lifecycle remain M3 requirements.
