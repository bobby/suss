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

The M0 policy/schema gate remains complete. The review overlay now records
seventeen in-progress declarations; 1,048 remain unassessed. Original runtime
intrinsics do not establish upstream form ports. The first reproducible bootstrap
source import selects `identity`, retaining its original form/notices and applying
an explicit reviewed defn adaptation. Its generated directory packages the
byte-preserved EPL files. See [core import](CORE-IMPORT.md) for commands, hashes,
executing scope and remaining M4 acceptance work. No complete core-port claim follows.
The legacy adapted conformance corpus retains its own
[attribution](../../reference/cljs-tests/README.md); it is a small curated baseline,
not proof of complete source provenance or upstream differential compatibility.
