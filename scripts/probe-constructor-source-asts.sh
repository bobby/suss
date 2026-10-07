#!/bin/sh
# Development-only exact raw analyzer and actual Node observations.
set -eu
constructor_ast_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
test "$(git -C "$constructor_ast_root/clojurescript" rev-parse HEAD)" = c4295f303100bbf5afac449242d30bca1126f1a1
cd "$constructor_ast_root/tests/oracle"
mkdir -p out
: > out/constructor-source-ast-calls.jsonl
CLJ_CONFIG=/tmp/suss-oracle-clojure-config CLJ_CACHE=/tmp/suss-oracle-clojure-cache \
clojure -Srepro -M -m cljs.main -co '{:force true :cache-analysis false :target :nodejs :output-to "out/constructor-source-asts.js" :output-dir "out/constructor-source-asts-cljs" :optimizations :none :source-map false}' -c suss-oracle.constructor-source-ast-runner
node out/constructor-source-asts.js > out/constructor-source-ast-results.json
