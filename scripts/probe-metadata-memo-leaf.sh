#!/bin/sh
# Development-only observations; never writes expected observations implicitly.
set -eu
memo_leaf_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
test "$(git -C "$memo_leaf_root/clojurescript" rev-parse HEAD)" = c4295f303100bbf5afac449242d30bca1126f1a1
test -z "$(git -C "$memo_leaf_root/clojurescript" status --porcelain)"
cd "$memo_leaf_root/tests/oracle"
mkdir -p out
CLJ_CONFIG=/tmp/suss-oracle-clojure-config CLJ_CACHE=/tmp/suss-oracle-clojure-cache \
clojure -J-Xmx1g -J-XX:ActiveProcessorCount=2 -Srepro -M -m cljs.main -co '{:force true :cache-analysis false :target :nodejs :output-to "out/metadata-memo-leaf.js" :output-dir "out/metadata-memo-leaf-cljs" :optimizations :none :source-map false}' -c suss-oracle.metadata-memo-leaf
node out/metadata-memo-leaf.js
