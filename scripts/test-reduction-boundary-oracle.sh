#!/bin/sh
# Development oracle only; schedule native Cargo separately.
set -eu
reduction_boundary_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
python3 "$reduction_boundary_root/scripts/oracle_cases.py"
python3 "$reduction_boundary_root/scripts/reduction_boundary_oracle.py" generate
cd "$reduction_boundary_root/tests/oracle"
CLJ_CONFIG=/tmp/suss-oracle-clojure-config CLJ_CACHE=/tmp/suss-oracle-clojure-cache \
clojure -Srepro -M -m cljs.main -co '{:target :nodejs :output-to "out/reduction-boundaries.js" :output-dir "out/reduction-boundary-cljs" :optimizations :none :source-map false}' -c suss-oracle.reduction-boundaries
node out/reduction-boundaries.js > out/reduction-boundary-observations.json
python3 "$reduction_boundary_root/scripts/reduction_boundary_oracle.py" compare
