#!/bin/sh
set -eu
record_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
test "$(git -C "$record_root/clojurescript" rev-parse HEAD)" = c4295f303100bbf5afac449242d30bca1126f1a1
python3 "$record_root/scripts/reify_prerequisite_oracle.py" generate
cd "$record_root/tests/oracle"
CLJ_CONFIG=/tmp/suss-oracle-clojure-config CLJ_CACHE=/tmp/suss-oracle-clojure-cache clojure -Srepro -M -m cljs.main -co '{:target :nodejs :output-to "out/reify-prerequisites.js" :output-dir "out/reify-prerequisites-cljs" :optimizations :none :source-map false :cache-analysis false}' -c suss-oracle.reify-prerequisites
node out/reify-prerequisites.js > out/reify-prerequisites-observations.json
python3 "$record_root/scripts/reify_prerequisite_oracle.py" compare
