#!/bin/sh
# Development-only pinned observations followed by independently decoded Suss.
set -eu
macro_map_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
python3 "$macro_map_root/scripts/oracle_cases.py"
python3 "$macro_map_root/scripts/macro_map_oracle.py" generate
cd "$macro_map_root/tests/oracle"
CLJ_CONFIG=/tmp/suss-oracle-clojure-config CLJ_CACHE=/tmp/suss-oracle-clojure-cache \
clojure -Srepro -M -m cljs.main -co '{:target :nodejs :output-to "out/macro-maps.js" :output-dir "out/macro-map-cljs" :optimizations :none :source-map false}' -c suss-oracle.macro-maps
node out/macro-maps.js > out/macro-map-observations.json
python3 "$macro_map_root/scripts/macro_map_oracle.py" compare
CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}" \
cargo test --manifest-path "$macro_map_root/Cargo.toml" -p suss-cli --test compiled_macro_maps --locked -- --test-threads=2
