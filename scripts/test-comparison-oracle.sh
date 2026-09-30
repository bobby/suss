#!/bin/sh
# Development-only pinned observations followed by independently decoded Suss.
set -eu
comparison_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
python3 "$comparison_root/scripts/oracle_cases.py"
python3 "$comparison_root/scripts/comparison_oracle.py" generate
cd "$comparison_root/tests/oracle"
CLJ_CONFIG=/tmp/suss-oracle-clojure-config CLJ_CACHE=/tmp/suss-oracle-clojure-cache \
clojure -Srepro -M -m cljs.main -co '{:target :nodejs :output-to "out/comparisons.js" :output-dir "out/comparison-cljs" :optimizations :none :source-map false}' -c suss-oracle.comparisons
node out/comparisons.js > out/comparison-observations.json
python3 "$comparison_root/scripts/comparison_oracle.py" compare
CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}" \
cargo test --manifest-path "$comparison_root/Cargo.toml" -p suss-cli --test portable_comparisons --locked -- --test-threads=2
