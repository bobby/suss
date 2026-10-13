#!/bin/sh
set -eu
sorted_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
test "$(git -C "$sorted_root/clojurescript" rev-parse HEAD)" = c4295f303100bbf5afac449242d30bca1126f1a1
python3 -B "$sorted_root/scripts/sorted_prerequisite_oracle.py" check
cd "$sorted_root/tests/oracle"
CLJ_CONFIG=/tmp/suss-oracle-clojure-config CLJ_CACHE=/tmp/suss-oracle-clojure-cache clojure -Srepro -M -m cljs.main -co '{:target :nodejs :output-to "out/sorted-prerequisites.js" :output-dir "out/sorted-prerequisites-cljs" :optimizations :none :source-map false :cache-analysis false}' -c suss-oracle.sorted-prerequisites
node out/sorted-prerequisites.js > out/sorted-prerequisite-observations.json
python3 -B "$sorted_root/scripts/sorted_prerequisite_oracle.py" compare
