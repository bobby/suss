#!/bin/sh
# Development-only: compile and execute pinned cross-invocation declaration identity.
set -eu
global_ast_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
test "$(git -C "$global_ast_root/clojurescript" rev-parse HEAD)" = c4295f303100bbf5afac449242d30bca1126f1a1
cd "$global_ast_root/tests/oracle"
mkdir -p out
: > out/global-reference-cross-invocation.jsonl
: > out/global-reference-info-identity.jsonl
CLJ_CONFIG=/tmp/suss-oracle-clojure-config CLJ_CACHE=/tmp/suss-oracle-clojure-cache \
clojure -Srepro -M -m cljs.main -co '{:force true :cache-analysis false :target :nodejs :output-to "out/global-reference-cross-invocation.js" :output-dir "out/global-reference-cross-invocation-cljs" :optimizations :none :source-map false}' -c suss-oracle.global-reference-identity-probe-runner
node out/global-reference-cross-invocation.js > out/global-reference-cross-invocation-results.json
python3 "$global_ast_root/scripts/global_reference_identity_oracle.py"
