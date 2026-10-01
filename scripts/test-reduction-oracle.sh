#!/bin/sh
# Development-only reference observations, then independently decoded native tests.
set -eu
reduction_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
python3 "$reduction_root/scripts/oracle_cases.py"
python3 "$reduction_root/scripts/reduction_oracle.py" generate
cd "$reduction_root/tests/oracle"
CLJ_CONFIG=/tmp/suss-oracle-clojure-config CLJ_CACHE=/tmp/suss-oracle-clojure-cache \
clojure -Srepro -M -m cljs.main -co '{:target :nodejs :output-to "out/reductions.js" :output-dir "out/reduction-cljs" :optimizations :none :source-map false}' -c suss-oracle.reductions
node out/reductions.js > out/reduction-observations.json
python3 "$reduction_root/scripts/reduction_oracle.py" compare
CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}" \
cargo test --manifest-path "$reduction_root/Cargo.toml" -p suss-cli --test portable_reduction --locked -- --test-threads=2
