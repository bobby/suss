# First-class inc/dec Array conversion evidence

Pinned ClojureScript `c4295f303100bbf5afac449242d30bca1126f1a1` independently confirms first-class `inc` on `(array 1)` produces String `"11"`, while `dec` produces Number `0`. Original reference fixture and compile log are retained in `evidence/`; both ordered boolean observations are true. This is a two-case regression reference, not complete object coercion coverage.

Reference command (from `tests/oracle`):
```sh
clojure -Sdeps '{:deps {org.clojure/clojurescript {:local/root "/private/tmp/suss-m4-queues/clojurescript"}}}' -M -m cljs.main -co '{:target :nodejs :output-to "out/array-inc-dec/main.js" :output-dir "out/array-inc-dec/compiled" :optimizations :none}' -c suss-oracle.array-inc-dec
node out/array-inc-dec/main.js
```

Native command: `CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR=/private/tmp/suss-m4-mlir-toolchain/native-target-post226 sh scripts/verify-core-import.sh`. All 17 core-import tests passed, including exact UTF16 String units and Number bits after GC. Unsupported Object/closure cases and subsequent recovery remain asserted. No test allowance changed. `evidence/files.json` records retained file hashes and sizes.
