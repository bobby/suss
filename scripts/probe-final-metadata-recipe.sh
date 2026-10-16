#!/bin/sh
# Development-only pinned CLJS analyzer/runtime metadata observations.
set -eu
metadata_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
test "$(git -C "$metadata_root/clojurescript" rev-parse HEAD)" = c4295f303100bbf5afac449242d30bca1126f1a1
test -z "$(git -C "$metadata_root/clojurescript" status --porcelain)"
python3 "$metadata_root/scripts/oracle_cases.py"
cd "$metadata_root/tests/oracle"
mkdir -p out
CLJ_CONFIG=/tmp/suss-oracle-clojure-config CLJ_CACHE=/tmp/suss-oracle-clojure-cache \
clojure -J-Xmx1g -J-XX:ActiveProcessorCount=2 -Srepro -M -m cljs.main -co '{:force true :cache-analysis false :target :nodejs :output-to "out/final-metadata-recipe.js" :output-dir "out/final-metadata-recipe-cljs" :optimizations :none :source-map false}' -c suss-oracle.final-metadata-recipe
node out/final-metadata-recipe.js
