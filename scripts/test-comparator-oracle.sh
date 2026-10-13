#!/bin/sh
set -eu
comparator_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
test "$(git -C "$comparator_root/clojurescript" rev-parse HEAD)" = c4295f303100bbf5afac449242d30bca1126f1a1
python3 -B "$comparator_root/scripts/comparator_oracle.py" check
cd "$comparator_root/tests/oracle"
CLJ_CONFIG=/tmp/suss-oracle-clojure-config CLJ_CACHE=/tmp/suss-oracle-clojure-cache clojure -Srepro -M -m cljs.main -co '{:target :nodejs :output-to "out/comparators.js" :output-dir "out/comparators-cljs" :optimizations :none :source-map false :cache-analysis false}' -c suss-oracle.comparators
node out/comparators.js > out/comparator-observations.json
python3 -B "$comparator_root/scripts/comparator_oracle.py" compare
