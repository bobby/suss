#!/bin/sh
# Development-only pinned observations followed by independently decoded Suss.
set -eu
array_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
python3 "$array_root/scripts/oracle_cases.py"
python3 "$array_root/scripts/array_oracle.py" generate
cd "$array_root/tests/oracle"
CLJ_CONFIG=/tmp/suss-oracle-clojure-config CLJ_CACHE=/tmp/suss-oracle-clojure-cache \
clojure -Srepro -M -m cljs.main -co '{:target :nodejs :output-to "out/arrays.js" :output-dir "out/array-cljs" :optimizations :none :source-map false}' -c suss-oracle.arrays
node out/arrays.js > out/array-observations.json
python3 "$array_root/scripts/array_oracle.py" compare
CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}" \
cargo test --manifest-path "$array_root/Cargo.toml" -p suss-cli --test portable_arrays --locked -- --test-threads=2
