#!/bin/sh
# Development-only candidate probe. Inspect fresh outputs before freezing a corpus.
set -eu
collection_ast_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
test "$(git -C "$collection_ast_root/clojurescript" rev-parse HEAD)" = c4295f303100bbf5afac449242d30bca1126f1a1
cd "$collection_ast_root/tests/oracle"
mkdir -p out
: > out/collection-source-ast-calls.jsonl
CLJ_CONFIG=/tmp/suss-oracle-clojure-config CLJ_CACHE=/tmp/suss-oracle-clojure-cache \
clojure -Srepro -M -m cljs.main -co '{:force true :cache-analysis false :target :nodejs :output-to "out/collection-source-asts.js" :output-dir "out/collection-source-asts-cljs" :optimizations :none :source-map false}' -c suss-oracle.collection-source-ast-runner
node out/collection-source-asts.js > out/collection-source-ast-results.json
