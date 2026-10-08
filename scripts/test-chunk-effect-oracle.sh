#!/bin/sh
# Pinned development oracle only. Native tests are separately scheduled; no Cargo.
set -eu
chunk_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
test "$(git -C "$chunk_root/clojurescript" rev-parse HEAD)" = c4295f303100bbf5afac449242d30bca1126f1a1
python3 "$chunk_root/scripts/oracle_cases.py"
python3 "$chunk_root/scripts/chunk_effect_oracle.py" generate
cd "$chunk_root/tests/oracle"
CLJ_CONFIG=/tmp/suss-oracle-clojure-config CLJ_CACHE=/tmp/suss-oracle-clojure-cache \
clojure -Srepro -M -m cljs.main -co '{:target :nodejs :output-to "out/chunk-effects.js" :output-dir "out/chunk-effect-cljs" :optimizations :none :source-map false :cache-analysis false :force true}' -c suss-oracle.chunk-effects
node out/chunk-effects.js > out/chunk-effect-observations.json
python3 "$chunk_root/scripts/chunk_effect_oracle.py" compare
