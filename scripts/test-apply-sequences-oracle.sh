#!/bin/sh
# Development-only pinned observations followed by independently decoded Suss.
set -eu
apply_sequences_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
python3 "$apply_sequences_root/scripts/oracle_cases.py"
python3 "$apply_sequences_root/scripts/apply_sequences_oracle.py" generate
cd "$apply_sequences_root/tests/oracle"
CLJ_CONFIG=/tmp/suss-oracle-clojure-config CLJ_CACHE=/tmp/suss-oracle-clojure-cache \
clojure -Srepro -M -m cljs.main -co '{:target :nodejs :output-to "out/apply-sequences.js" :output-dir "out/apply-sequences-cljs" :optimizations :none :source-map false}' -c suss-oracle.apply-sequences
node out/apply-sequences.js > out/apply-sequences-observations.json
python3 "$apply_sequences_root/scripts/apply_sequences_oracle.py" compare
CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}" \
cargo test --manifest-path "$apply_sequences_root/Cargo.toml" -p suss-cli --test portable_apply_sequences --locked -- --test-threads=2
