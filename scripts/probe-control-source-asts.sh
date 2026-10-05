#!/bin/sh
# Development-only candidate probe. Inspect fresh outputs before freezing a corpus.
set -eu
control_ast_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
test "$(git -C "$control_ast_root/clojurescript" rev-parse HEAD)" = c4295f303100bbf5afac449242d30bca1126f1a1
cd "$control_ast_root/tests/oracle"
mkdir -p out
: > out/control-source-ast-calls.jsonl
CLJ_CONFIG=/tmp/suss-oracle-clojure-config CLJ_CACHE=/tmp/suss-oracle-clojure-cache \
clojure -Srepro -M -m cljs.main -co '{:force true :cache-analysis false :target :nodejs :output-to "out/control-source-asts.js" :output-dir "out/control-source-asts-cljs" :optimizations :none :source-map false}' -c suss-oracle.control-source-ast-runner
node out/control-source-asts.js > out/control-source-ast-results.json
