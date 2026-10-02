#!/bin/sh
# Development-only pinned observations followed by independently decoded Suss.
set -eu
hash_map_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
python3 "$hash_map_root/scripts/oracle_cases.py"
python3 "$hash_map_root/scripts/hash_map_oracle.py" generate
cd "$hash_map_root/tests/oracle"
CLJ_CONFIG=/tmp/suss-oracle-clojure-config CLJ_CACHE=/tmp/suss-oracle-clojure-cache \
clojure -Srepro -M -m cljs.main -co '{:target :nodejs :output-to "out/hash-map.js" :output-dir "out/hash-map-cljs" :optimizations :none :source-map false}' -c suss-oracle.hash-map
node out/hash-map.js > out/hash-map-observations.json
python3 "$hash_map_root/scripts/hash_map_oracle.py" compare
CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}" \
cargo test --manifest-path "$hash_map_root/Cargo.toml" -p suss-cli --test compiled_macro_hash_maps --locked -- --test-threads=2
