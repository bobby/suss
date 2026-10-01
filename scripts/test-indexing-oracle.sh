#!/bin/sh
# Development-only reference observations, then independently decoded native tests.
set -eu
indexing_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
python3 "$indexing_root/scripts/oracle_cases.py"
python3 "$indexing_root/scripts/indexing_oracle.py" generate
cd "$indexing_root/tests/oracle"
CLJ_CONFIG=/tmp/suss-oracle-clojure-config CLJ_CACHE=/tmp/suss-oracle-clojure-cache \
clojure -Srepro -M -m cljs.main -co '{:target :nodejs :output-to "out/indexing.js" :output-dir "out/indexing-cljs" :optimizations :none :source-map false}' -c suss-oracle.indexing
node out/indexing.js > out/indexing-observations.json
python3 "$indexing_root/scripts/indexing_oracle.py" compare
CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}" \
cargo test --manifest-path "$indexing_root/Cargo.toml" -p suss-cli --test portable_indexing --locked -- --test-threads=2
