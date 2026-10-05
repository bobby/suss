#!/bin/sh
# Development-only candidate probe. Inspect fresh outputs before freezing a corpus.
set -eu
function_name_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
test "$(git -C "$function_name_root/clojurescript" rev-parse HEAD)" = c4295f303100bbf5afac449242d30bca1126f1a1
cd "$function_name_root/tests/oracle"
mkdir -p out
: > out/function-name-ast-calls.jsonl
CLJ_CONFIG=/tmp/suss-oracle-clojure-config CLJ_CACHE=/tmp/suss-oracle-clojure-cache \
clojure -Srepro -M -m cljs.main -co '{:force true :cache-analysis false :target :nodejs :output-to "out/function-name-asts.js" :output-dir "out/function-name-asts-cljs" :optimizations :none :source-map false}' -c suss-oracle.function-name-ast-runner
node out/function-name-asts.js > out/function-name-ast-results.json
