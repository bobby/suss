#!/bin/sh
# Development-only reference observations, then independently decoded native tests.
set -eu
public_hash_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
python3 "$public_hash_root/scripts/oracle_cases.py"
python3 "$public_hash_root/scripts/public_hash_oracle.py" generate
cd "$public_hash_root/tests/oracle"
CLJ_CONFIG=/tmp/suss-oracle-clojure-config CLJ_CACHE=/tmp/suss-oracle-clojure-cache \
clojure -Srepro -M -m cljs.main -co '{:target :nodejs :output-to "out/public-hash.js" :output-dir "out/public-hash-cljs" :optimizations :none :source-map false}' -c suss-oracle.public-hash
node out/public-hash.js > out/public-hash-observations.json
python3 "$public_hash_root/scripts/public_hash_oracle.py" compare
CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}" \
cargo test --manifest-path "$public_hash_root/Cargo.toml" -p suss-cli --test portable_public_hash --locked -- --test-threads=2
