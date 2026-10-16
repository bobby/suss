#!/bin/sh
# Development-only CLJS-hosted analyzer observation; no native expectation rewrite.
set -eu
retention_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
test "$(git -C "$retention_root/clojurescript" rev-parse HEAD)" = c4295f303100bbf5afac449242d30bca1126f1a1
test -z "$(git -C "$retention_root/clojurescript" status --porcelain)"
cd "$retention_root/tests/oracle"
mkdir -p out
CLJ_CONFIG=/tmp/suss-oracle-clojure-config CLJ_CACHE=/tmp/suss-oracle-clojure-cache \
clojure -J-Xmx1g -J-XX:ActiveProcessorCount=2 -Srepro -M -m cljs.main -co '{:force true :cache-analysis false :target :nodejs :output-to "out/declaration-metadata-retention.js" :output-dir "out/declaration-metadata-retention-cljs" :optimizations :none :source-map false}' -c suss-oracle.declaration-metadata-retention
node out/declaration-metadata-retention.js
