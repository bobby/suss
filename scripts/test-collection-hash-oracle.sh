#!/bin/sh
# Development-only reference observations, then independently decoded native tests.
set -eu
collection_hash_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
python3 "$collection_hash_root/scripts/oracle_cases.py"
python3 "$collection_hash_root/scripts/collection_hash_oracle.py" generate
cd "$collection_hash_root/tests/oracle"
CLJ_CONFIG=/tmp/suss-oracle-clojure-config CLJ_CACHE=/tmp/suss-oracle-clojure-cache \
clojure -Srepro -M -m cljs.main -co '{:target :nodejs :output-to "out/collection-hash.js" :output-dir "out/collection-hash-cljs" :optimizations :none :source-map false}' -c suss-oracle.collection-hash
node out/collection-hash.js > out/collection-hash-observations.json
python3 "$collection_hash_root/scripts/collection_hash_oracle.py" compare
CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}" \
cargo test --manifest-path "$collection_hash_root/Cargo.toml" -p suss-cli --test portable_collection_hash --locked -- --test-threads=2
