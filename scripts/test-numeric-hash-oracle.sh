#!/bin/sh
# Development-only reference observations, then independently decoded native tests.
set -eu
hash_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
python3 "$hash_root/scripts/oracle_cases.py"
python3 "$hash_root/scripts/numeric_hash_oracle.py" generate
cd "$hash_root/tests/oracle"
CLJ_CONFIG=/tmp/suss-oracle-clojure-config CLJ_CACHE=/tmp/suss-oracle-clojure-cache \
clojure -Srepro -M -m cljs.main -co '{:target :nodejs :output-to "out/numeric-hash.js" :output-dir "out/numeric-hash-cljs" :optimizations :none :source-map false}' -c suss-oracle.numeric-hash
node out/numeric-hash.js > out/numeric-hash-observations.json
python3 "$hash_root/scripts/numeric_hash_oracle.py" compare
CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}" \
cargo test --manifest-path "$hash_root/Cargo.toml" -p suss-cli --test portable_numeric_hash --locked -- --test-threads=2
