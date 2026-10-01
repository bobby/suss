#!/bin/sh
# Development-only reference observations, then independently decoded native tests.
set -eu
hash_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
python3 "$hash_root/scripts/oracle_cases.py"
python3 "$hash_root/scripts/bitwise_hash_oracle.py" generate
cd "$hash_root/tests/oracle"
CLJ_CONFIG=/tmp/suss-oracle-clojure-config CLJ_CACHE=/tmp/suss-oracle-clojure-cache \
clojure -Srepro -M -m cljs.main -co '{:target :nodejs :output-to "out/bitwise-hash.js" :output-dir "out/bitwise-hash-cljs" :optimizations :none :source-map false}' -c suss-oracle.bitwise-hash
node out/bitwise-hash.js > out/bitwise-hash-observations.json
python3 "$hash_root/scripts/bitwise_hash_oracle.py" compare
python3 "$hash_root/scripts/bitwise_capture_oracle.py" generate
CLJ_CONFIG=/tmp/suss-oracle-clojure-config CLJ_CACHE=/tmp/suss-oracle-clojure-cache \
clojure -Srepro -M -m cljs.main -co '{:target :nodejs :output-to "out/bitwise-capture.js" :output-dir "out/bitwise-capture-cljs" :optimizations :none :source-map false}' -c suss-oracle.bitwise-capture
node out/bitwise-capture.js > out/bitwise-capture-observations.json
python3 "$hash_root/scripts/bitwise_capture_oracle.py" compare
CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}" \
cargo test --manifest-path "$hash_root/Cargo.toml" -p suss-cli --test portable_bitwise_hash --locked -- --test-threads=2
