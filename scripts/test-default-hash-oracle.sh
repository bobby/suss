#!/bin/sh
# Development-only reference observations, then independently decoded native tests.
set -eu
default_hash_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
python3 "$default_hash_root/scripts/oracle_cases.py"
python3 "$default_hash_root/scripts/default_hash_oracle.py" generate
cd "$default_hash_root/tests/oracle"
CLJ_CONFIG=/tmp/suss-oracle-clojure-config CLJ_CACHE=/tmp/suss-oracle-clojure-cache \
clojure -Srepro -M -m cljs.main -co '{:target :nodejs :output-to "out/default-hash.js" :output-dir "out/default-hash-cljs" :optimizations :none :source-map false}' -c suss-oracle.default-hash
node out/default-hash.js > out/default-hash-observations.json
python3 "$default_hash_root/scripts/default_hash_oracle.py" compare
CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}" \
cargo test --manifest-path "$default_hash_root/Cargo.toml" -p suss-cli --test portable_default_hash --locked -- --test-threads=2
