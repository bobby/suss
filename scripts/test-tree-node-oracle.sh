#!/bin/sh
set -eu
node_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
test "$(git -C "$node_root/clojurescript" rev-parse HEAD)" = c4295f303100bbf5afac449242d30bca1126f1a1
python3 -B "$node_root/scripts/tree_node_oracle.py" check
cd "$node_root/tests/oracle"
CLJ_CONFIG=/tmp/suss-oracle-clojure-config CLJ_CACHE=/tmp/suss-oracle-clojure-cache clojure -Srepro -M -m cljs.main -co '{:target :nodejs :output-to "out/tree-nodes.js" :output-dir "out/tree-nodes-cljs" :optimizations :none :source-map false :cache-analysis false}' -c suss-oracle.tree-nodes
node out/tree-nodes.js > out/tree-node-observations.json
python3 -B "$node_root/scripts/tree_node_oracle.py" compare
