#!/bin/sh
# Development-only: compile and execute pinned source-global projections.
set -eu
global_ast_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
test "$(git -C "$global_ast_root/clojurescript" rev-parse HEAD)" = c4295f303100bbf5afac449242d30bca1126f1a1
cd "$global_ast_root/tests/oracle"
mkdir -p out
: > out/global-reference-ast-calls.jsonl
CLJ_CONFIG=/tmp/suss-oracle-clojure-config CLJ_CACHE=/tmp/suss-oracle-clojure-cache \
clojure -Srepro -M -m cljs.main -co '{:force true :cache-analysis false :target :nodejs :output-to "out/global-reference-asts.js" :output-dir "out/global-reference-asts-cljs" :optimizations :none :source-map false}' -c suss-oracle.global-reference-ast-runner
node out/global-reference-asts.js > out/global-reference-ast-results.json
python3 "$global_ast_root/scripts/global_reference_asts_oracle.py"
