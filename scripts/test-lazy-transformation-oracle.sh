#!/bin/sh
# Development oracle only. Native artifacts are a separate gate; no Cargo.
set -eu
lazy_transform_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
python3 "$lazy_transform_root/scripts/cljs_inventory.py" --check
python3 "$lazy_transform_root/scripts/oracle_cases.py"
python3 "$lazy_transform_root/scripts/lazy_transformation_oracle.py" generate
cd "$lazy_transform_root/tests/oracle"
CLJ_CONFIG=/tmp/suss-oracle-clojure-config CLJ_CACHE=/tmp/suss-oracle-clojure-cache \
clojure -Srepro -M -m cljs.main -co '{:force true :cache-analysis false :target :nodejs :output-to "out/lazy-transformations.js" :output-dir "out/lazy-transformation-cljs" :optimizations :none :source-map false}' -c suss-oracle.lazy-transformations
node out/lazy-transformations.js > out/lazy-transformation-observations.json
python3 "$lazy_transform_root/scripts/lazy_transformation_oracle.py" compare
