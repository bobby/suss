#!/bin/sh
# Pinned ClojureScript capture for the shared public-record nominal contract.
set -eu
record_nominal_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
record_nominal_pin=c4295f303100bbf5afac449242d30bca1126f1a1
if [ "$(git -C "$record_nominal_root/clojurescript" rev-parse HEAD)" != "$record_nominal_pin" ]; then
  echo "record nominal oracle requires the exact pinned ClojureScript checkout" >&2
  exit 1
fi
if [ -n "$(git -C "$record_nominal_root/clojurescript" status --porcelain)" ]; then
  echo "record nominal oracle source checkout must be clean" >&2
  exit 1
fi
python3 "$record_nominal_root/scripts/record_nominal_oracle.py" generate
cd "$record_nominal_root/tests/oracle"
CLJ_CONFIG=/tmp/suss-oracle-clojure-config CLJ_CACHE=/tmp/suss-oracle-clojure-cache \
clojure -Srepro -M -m cljs.main -co '{:target :nodejs :output-to "out/record-nominal.js" :output-dir "out/record-nominal-cljs" :optimizations :none :source-map false :cache-analysis false :force true}' -c app-records
node out/record-nominal.js > out/record-nominal-observations.json
python3 "$record_nominal_root/scripts/record_nominal_oracle.py" compare
