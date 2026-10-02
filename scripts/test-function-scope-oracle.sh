#!/bin/sh
# Development-only compiler function-name facts.
set -eu
scope_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$scope_root/tests/oracle"
CLJ_CONFIG=/tmp/suss-oracle-clojure-config CLJ_CACHE=/tmp/suss-oracle-clojure-cache \
clojure -Srepro -M -m cljs.main -co '{:force true :cache-analysis false :target :nodejs :output-to "out/function-scope.js" :output-dir "out/function-scope-cljs" :optimizations :none :source-map false}' -c suss-oracle.function-scope-facts
node out/function-scope.js > out/function-scope-results.json
python3 "$scope_root/scripts/function_scope_oracle.py"
CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}" cargo test --manifest-path "$scope_root/Cargo.toml" -p suss-cli --locked --test compiled_macro_function_scopes -- --test-threads=2
