#!/bin/sh
# Development-only oracle: actual source macro invocation and Node execution.
set -eu
source_env_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
test "$(git -C "$source_env_root/clojurescript" rev-parse HEAD)" = c4295f303100bbf5afac449242d30bca1126f1a1
cd "$source_env_root/tests/oracle"
mkdir -p out
: > out/source-environment-calls.jsonl
CLJ_CONFIG=/tmp/suss-oracle-clojure-config CLJ_CACHE=/tmp/suss-oracle-clojure-cache \
clojure -Srepro -M -m cljs.main -co '{:force true :cache-analysis false :target :nodejs :output-to "out/source-environment.js" :output-dir "out/source-environment-cljs" :optimizations :none :source-map false}' -c suss-oracle.source-environment-runner
node out/source-environment.js > out/source-environment-result.json
python3 "$source_env_root/scripts/source_environment_oracle.py"
