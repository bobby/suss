#!/bin/sh
# Development-only reference observations, then independently decoded native tests.
set -eu
identity_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
python3 "$identity_root/scripts/oracle_cases.py"
python3 "$identity_root/scripts/identity_hash_oracle.py" generate
cd "$identity_root/tests/oracle"
CLJ_CONFIG=/tmp/suss-oracle-clojure-config CLJ_CACHE=/tmp/suss-oracle-clojure-cache \
clojure -Srepro -M -m cljs.main -co '{:target :nodejs :output-to "out/identity-hash.js" :output-dir "out/identity-hash-cljs" :optimizations :none :source-map false}' -c suss-oracle.identity-hash
node out/identity-hash.js > out/identity-hash-observations.json
python3 "$identity_root/scripts/identity_hash_oracle.py" compare
CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}" \
cargo test --manifest-path "$identity_root/Cargo.toml" -p suss-cli --test portable_identity_hash --locked -- --test-threads=2
