#!/bin/sh
# Development-only: compile and execute the pinned field reference projections.
set -eu
field_ast_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
test "$(git -C "$field_ast_root/clojurescript" rev-parse HEAD)" = c4295f303100bbf5afac449242d30bca1126f1a1
cd "$field_ast_root/tests/oracle"
mkdir -p out
: > out/field-reference-ast-calls.jsonl
CLJ_CONFIG=/tmp/suss-oracle-clojure-config CLJ_CACHE=/tmp/suss-oracle-clojure-cache \
clojure -Srepro -M -m cljs.main -co '{:force true :cache-analysis false :target :nodejs :output-to "out/field-reference-asts.js" :output-dir "out/field-reference-asts-cljs" :optimizations :none :source-map false}' -c suss-oracle.field-reference-ast-runner
node out/field-reference-asts.js > out/field-reference-ast-results.json
python3 "$field_ast_root/scripts/field_reference_asts_oracle.py"
