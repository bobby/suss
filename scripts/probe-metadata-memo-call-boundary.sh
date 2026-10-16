#!/bin/sh
# Development-only pinned CLJS protocol/metadata observation; no expectation rewrite.
set -eu
memo_boundary_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
test "$(git -C "$memo_boundary_root/clojurescript" rev-parse HEAD)" = c4295f303100bbf5afac449242d30bca1126f1a1
test -z "$(git -C "$memo_boundary_root/clojurescript" status --porcelain)"
cd "$memo_boundary_root/tests/oracle"
mkdir -p out
CLJ_CONFIG=/tmp/suss-oracle-clojure-config CLJ_CACHE=/tmp/suss-oracle-clojure-cache \
clojure -J-Xmx1g -J-XX:ActiveProcessorCount=2 -Srepro -M -m cljs.main -co '{:force true :cache-analysis false :target :nodejs :output-to "out/metadata-memo-call-boundary.js" :output-dir "out/metadata-memo-call-boundary-cljs" :optimizations :none :source-map false}' -c suss-oracle.metadata-memo-call-boundary
node out/metadata-memo-call-boundary.js
