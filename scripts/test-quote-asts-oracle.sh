#!/bin/sh
# Development-only: compile and execute pinned quoted-source projections.
set -eu
quote_ast_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
test "$(git -C "$quote_ast_root/clojurescript" rev-parse HEAD)" = c4295f303100bbf5afac449242d30bca1126f1a1
cd "$quote_ast_root/tests/oracle"
mkdir -p out
: > out/quote-ast-calls.jsonl
CLJ_CONFIG=/tmp/suss-oracle-clojure-config CLJ_CACHE=/tmp/suss-oracle-clojure-cache \
clojure -Srepro -M -m cljs.main -co '{:force true :cache-analysis false :target :nodejs :output-to "out/quote-asts.js" :output-dir "out/quote-asts-cljs" :optimizations :none :source-map false}' -c suss-oracle.quote-ast-runner
node out/quote-asts.js > out/quote-ast-results.json
python3 "$quote_ast_root/scripts/quote_asts_oracle.py"
