#!/bin/sh
# Development-only: compile and execute the pinned local reference projections.
set -eu
local_ast_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
test "$(git -C "$local_ast_root/clojurescript" rev-parse HEAD)" = c4295f303100bbf5afac449242d30bca1126f1a1
cd "$local_ast_root/tests/oracle"
mkdir -p out
: > out/local-reference-ast-calls.jsonl
CLJ_CONFIG=/tmp/suss-oracle-clojure-config CLJ_CACHE=/tmp/suss-oracle-clojure-cache \
clojure -Srepro -M -m cljs.main -co '{:force true :cache-analysis false :target :nodejs :output-to "out/local-reference-asts.js" :output-dir "out/local-reference-asts-cljs" :optimizations :none :source-map false}' -c suss-oracle.local-reference-ast-runner
node out/local-reference-asts.js > out/local-reference-ast-results.json
python3 "$local_ast_root/scripts/local_reference_asts_oracle.py"
