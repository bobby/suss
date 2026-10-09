#!/bin/sh
set -eu
helper_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
test "$(git -C "$helper_root/clojurescript" rev-parse HEAD)" = c4295f303100bbf5afac449242d30bca1126f1a1
python3 -B "$helper_root/scripts/record_helper_oracle.py" generate
cd "$helper_root/tests/oracle"
CLJ_CONFIG=/tmp/suss-oracle-clojure-config CLJ_CACHE=/tmp/suss-oracle-clojure-cache clojure -Srepro -M -m cljs.main -co '{:target :nodejs :output-to "out/record-helpers.js" :output-dir "out/record-helpers-cljs" :optimizations :none :source-map false :cache-analysis false}' -c suss-oracle.record-helpers
node out/record-helpers.js > out/record-helper-observations.json
python3 -B "$helper_root/scripts/record_helper_oracle.py" compare
