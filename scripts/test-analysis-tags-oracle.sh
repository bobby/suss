#!/bin/sh
# Development-only facts: force actual pinned analysis and execute the artifact.
set -eu
analysis_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
test "$(git -C "$analysis_root/clojurescript" rev-parse HEAD)" = c4295f303100bbf5afac449242d30bca1126f1a1
cd "$analysis_root/tests/oracle"
mkdir -p out
: > out/analysis-tag-calls.jsonl
CLJ_CONFIG=/tmp/suss-oracle-clojure-config CLJ_CACHE=/tmp/suss-oracle-clojure-cache \
clojure -Srepro -M -m cljs.main -co '{:force true :cache-analysis false :target :nodejs :output-to "out/analysis-tags.js" :output-dir "out/analysis-tags-cljs" :optimizations :none :source-map false}' -c suss-oracle.analysis-tag-runner
node out/analysis-tags.js > out/analysis-tag-results.json
python3 "$analysis_root/scripts/analysis_tags_oracle.py"
