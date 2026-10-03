#!/bin/sh
# Development-only declaration facts, with fresh analysis and real Node execution.
set -eu
declaration_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
test "$(git -C "$declaration_root/clojurescript" rev-parse HEAD)" = c4295f303100bbf5afac449242d30bca1126f1a1
cd "$declaration_root/tests/oracle"
mkdir -p out
: > out/declaration-environment-calls.jsonl
CLJ_CONFIG=/tmp/suss-oracle-clojure-config CLJ_CACHE=/tmp/suss-oracle-clojure-cache \
clojure -Srepro -M -m cljs.main -co '{:force true :cache-analysis false :target :nodejs :output-to "out/declaration-environment.js" :output-dir "out/declaration-environment-cljs" :optimizations :none :source-map false}' -c suss-oracle.declaration-runner
node out/declaration-environment.js > out/declaration-environment-result.json
python3 "$declaration_root/scripts/declaration_environment_oracle.py"
