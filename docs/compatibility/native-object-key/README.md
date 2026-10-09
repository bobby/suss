# Ordinary object property keys — focused baseline repair

The frozen 3a50acb full baseline (session94535) exited101, with502passed/1failed/0ignored. `private_object_errors_are_atomic_or_runtime_and_recover` incorrectly expected `(suss.bootstrap/object-set (factory) (factory) 7)` to throw. A source-labelled focused diagnostic reproduced that exact failure (exit101,1passed/1failed).

Fresh ClojureScript c4295f303100bbf5afac449242d30bca1126f1a1 compilation and Node execution confirm an ordinary object key becomes `[object Object]`; assignment returns7 and that named property contains7. The oracle uses public `js-obj` and explicit JavaScript bracket assignment/access to witness the private adapter contract. Hook observations independently retain numeric results: source operands precede string-hint conversion (trace1234), toString precedes valueOf, original thrown payload73 is preserved, prior property23 survives, and subsequent recovery returns19. It imports upstream under its existing EPL license; the new fixture is authored, not copied upstream source.

Reproduce from `tests/oracle` with an independent checkout at the exact pin:

```
CLJ_CONFIG=/tmp/suss-oracle-clojure-config CLJ_CACHE=/tmp/suss-oracle-clojure-cache clojure -Srepro -Sdeps '{:deps {org.clojure/clojurescript {:local/root "/private/tmp/suss-m4-pr230-late-ci/clojurescript"}}}' -M -m cljs.main -co '{:target :nodejs :output-to "out/native-object-key.js" :output-dir "out/native-object-key-cljs" :optimizations :none :source-map false :cache-analysis false}' -c suss-oracle.native-object-key
node out/native-object-key.js
```

Both exited0. `evidence/observations.json` retains raw numbers, not guest Boolean assertions. `evidence/receipt.json` records hashes of the fixture, generated entry, logs and baseline. The pinned oracle is development evidence, not native proof.

Native proof is the separate focused run, exit0,3passed/0failed/0ignored:

```
CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR=/private/tmp/suss-m4-mlir-toolchain/native-target-post226 cargo test -p suss-cli --test portable_native_objects --locked -- --test-threads=2
```

The original two tests remain, including nil-owner access, self/mutual cyclic factory errors, atomic compile failures and recovery. The additional test runs in both caller phases and reads boxed Number f64 bits after GC for assignment/property values, hook traces, thrown payload and recovery. No production source, bootstrap artifact or semantics changed. Full repaired baseline, final-head CI and independent review remain required; no bootstrap regeneration was needed for a test/evidence-only repair.
