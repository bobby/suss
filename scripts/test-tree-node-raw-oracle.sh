#!/bin/sh
set -eu
node_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
test "$(git -C "$node_root/clojurescript" rev-parse HEAD)" = c4295f303100bbf5afac449242d30bca1126f1a1
python3 -B "$node_root/scripts/tree_node_raw_oracle.py" check
cd "$node_root/tests/oracle"
CLJ_CONFIG=/tmp/suss-oracle-clojure-config CLJ_CACHE=/tmp/suss-oracle-clojure-cache clojure -Srepro -M -m cljs.main -co '{:target :nodejs :output-to "out/tree-node-raw.js" :output-dir "out/tree-node-raw-cljs" :optimizations :none :source-map false :cache-analysis false}' -c suss-oracle.tree-node-raw
node out/tree-node-raw.js > out/tree-node-raw-observations.json
python3 -B "$node_root/scripts/tree_node_raw_oracle.py" compare
