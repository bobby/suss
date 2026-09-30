# Core source and license policy

The compatibility source is `clojurescript/` at commit
`c4295f303100bbf5afac449242d30bca1126f1a1` (ClojureScript 1.12.134). The source files
`src/main/cljs/cljs/core.cljs` and `src/main/clojure/cljs/core.cljc` retain Rich
Hickey's copyright and Eclipse Public License 1.0 notices. The upstream license
texts remain in [LICENSE](../../clojurescript/LICENSE) and
[epl-v10.html](../../clojurescript/epl-v10.html). The generated inventory records
form ranges and hashes; it contains no classification or implementation inference.

Before porting a core form in M4, record its reviewed declaration ID, upstream
commit/path/range, source hash, extraction tool/version and unmodified extracted
form hash. Record adaptation patch paths and hashes, adapted artifact hash, phase,
dependencies and executing semantic test references. Regeneration must reproduce
the reviewed extraction and patches from that pin. Do not delete text merely
because it looks like JS interop; choose intrinsics and adaptations from the
portable ClojureScript contract.

Retain the upstream copyright/license notice on copied or adapted source and
include the corresponding EPL text with the distributed source/artifact. Do not
relabel upstream forms as repository MIT/Apache code. Any separately sourced
runtime or fixture retains its own license and provenance. Verify license
packaging and the source adaptation record before declaring a port complete.

An unreviewed declaration remains unassessed. An exclusion needs an individual
host-specific rationale and an explicit alternative in the review overlay.
Source hash changes invalidate its prior review. These rules apply to public and
private supporting forms, macros, protocols, generated constructors and both
reader branches. The scanner's source declarations are the starting inventory;
manual review must account for macro-generated public APIs.

This is the M0 policy and schema gate, not a completed core extraction system.
The current review overlay records nine arithmetic/nominal declarations as adapted and
in progress; 1,056 declarations remain unassessed. These original runtime
intrinsics do not establish completed upstream form ports. Reproducible extraction,
patch verification and shipped core-form license packaging are M4 acceptance work.
The legacy adapted conformance corpus retains its own
[attribution](../../reference/cljs-tests/README.md); it is a small curated baseline,
not proof of complete source provenance or upstream differential compatibility.
