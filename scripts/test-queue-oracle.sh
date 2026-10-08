#!/bin/sh
# Pinned development oracle; no native acceptance inferred.
set -eu
queue_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
test "$(git -C "$queue_root/clojurescript" rev-parse HEAD)" = c4295f303100bbf5afac449242d30bca1126f1a1
python3 "$queue_root/scripts/queue_oracle.py" generate
cd "$queue_root/tests/oracle"
for queue_name in queue-foundations queue-iterator; do
 CLJ_CONFIG=/tmp/suss-oracle-clojure-config CLJ_CACHE=/tmp/suss-oracle-clojure-cache clojure -Srepro -M -m cljs.main -co "{:target :nodejs :output-to \"out/$queue_name.js\" :output-dir \"out/$queue_name-cljs\" :optimizations :none :source-map false :cache-analysis false}" -c "suss-oracle.$queue_name"
 node "out/$queue_name.js" > "out/$queue_name-observations.json"
done
python3 "$queue_root/scripts/queue_oracle.py" compare
