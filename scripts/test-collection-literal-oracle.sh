#!/bin/sh
# Development-only reference observations, then independently decoded native tests.
set -eu
collection_literals_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
python3 "$collection_literals_root/scripts/oracle_cases.py"
python3 "$collection_literals_root/scripts/collection_literal_oracle.py" generate
cd "$collection_literals_root/tests/oracle"
CLJ_CONFIG=/tmp/suss-oracle-clojure-config CLJ_CACHE=/tmp/suss-oracle-clojure-cache \
clojure -Srepro -M -m cljs.main -co '{:target :nodejs :output-to "out/collection-literal.js" :output-dir "out/collection-literal-cljs" :optimizations :none :source-map false}' -c suss-oracle.collection-literal
node out/collection-literal.js > out/collection-literal-observations.json
python3 "$collection_literals_root/scripts/collection_literal_oracle.py" compare
CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}" \
cargo test --manifest-path "$collection_literals_root/Cargo.toml" -p suss-cli --test portable_collection_literals --locked -- --test-threads=2
