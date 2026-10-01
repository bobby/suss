#!/bin/sh
# Development-only reference observations, then independently decoded native tests.
set -eu
hash_numeric_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
python3 "$hash_numeric_root/scripts/oracle_cases.py"
python3 "$hash_numeric_root/scripts/hash_numeric_boundaries_oracle.py" generate
cd "$hash_numeric_root/tests/oracle"
CLJ_CONFIG=/tmp/suss-oracle-clojure-config CLJ_CACHE=/tmp/suss-oracle-clojure-cache \
clojure -Srepro -M -m cljs.main -co '{:target :nodejs :output-to "out/hash-numeric-boundaries.js" :output-dir "out/hash-numeric-boundaries-cljs" :optimizations :none :source-map false}' -c suss-oracle.hash-numeric-boundaries
node out/hash-numeric-boundaries.js > out/hash-numeric-boundaries-observations.json
python3 "$hash_numeric_root/scripts/hash_numeric_boundaries_oracle.py" compare
CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}" \
cargo test --manifest-path "$hash_numeric_root/Cargo.toml" -p suss-cli --test portable_hash_numeric_boundaries --locked -- --test-threads=2
