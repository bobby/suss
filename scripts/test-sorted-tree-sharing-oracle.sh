#!/bin/sh
set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
test "$(git -C "$root/clojurescript" rev-parse HEAD)" = c4295f303100bbf5afac449242d30bca1126f1a1
cd "$root/tests/oracle"
CLJ_CONFIG=/tmp/suss-oracle-clojure-config CLJ_CACHE=/tmp/suss-oracle-clojure-cache clojure -Srepro -M -m cljs.main -co '{:target :nodejs :output-to "out/sorted-tree-sharing.js" :output-dir "out/sorted-tree-sharing-cljs" :optimizations :none :source-map false :cache-analysis false}' -c suss-oracle.sorted-tree-sharing
node out/sorted-tree-sharing.js > out/sorted-tree-sharing.json
python3 -B "$root/scripts/sorted_tree_sharing_oracle.py"
