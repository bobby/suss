#!/bin/sh
# Development-only pinned observations followed by independently decoded Suss.
set -eu
macro_vector_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
python3 "$macro_vector_root/scripts/oracle_cases.py"
python3 "$macro_vector_root/scripts/macro_vector_oracle.py" generate
cd "$macro_vector_root/tests/oracle"
CLJ_CONFIG=/tmp/suss-oracle-clojure-config CLJ_CACHE=/tmp/suss-oracle-clojure-cache \
clojure -Srepro -M -m cljs.main -co '{:target :nodejs :output-to "out/macro-vectors.js" :output-dir "out/macro-vector-cljs" :optimizations :none :source-map false}' -c suss-oracle.macro-vectors
node out/macro-vectors.js > out/macro-vector-observations.json
python3 "$macro_vector_root/scripts/macro_vector_oracle.py" compare
CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}" \
cargo test --manifest-path "$macro_vector_root/Cargo.toml" -p suss-cli --test compiled_macro_vectors --locked -- --test-threads=2
