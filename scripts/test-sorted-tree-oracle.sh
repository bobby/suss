#!/bin/sh
set -eu
node_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
test "$(git -C "$node_root/clojurescript" rev-parse HEAD)" = c4295f303100bbf5afac449242d30bca1126f1a1
python3 -B "$node_root/scripts/sorted_tree_oracle.py" check
cd "$node_root/tests/oracle"
CLJ_CONFIG=/tmp/suss-oracle-clojure-config CLJ_CACHE=/tmp/suss-oracle-clojure-cache clojure -Srepro -M -m cljs.main -co '{:target :nodejs :output-to "out/sorted-tree.js" :output-dir "out/sorted-tree-cljs" :optimizations :none :source-map false :cache-analysis false}' -c suss-oracle.sorted-tree
node out/sorted-tree.js > out/sorted-tree-observations.json
python3 -B "$node_root/scripts/sorted_tree_oracle.py" compare
