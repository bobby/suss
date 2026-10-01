#!/bin/sh
# Development-only reference observations, then independently decoded native tests.
set -eu
sequence_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
python3 "$sequence_root/scripts/oracle_cases.py"
python3 "$sequence_root/scripts/sequence_oracle.py" generate
cd "$sequence_root/tests/oracle"
CLJ_CONFIG=/tmp/suss-oracle-clojure-config CLJ_CACHE=/tmp/suss-oracle-clojure-cache \
clojure -Srepro -M -m cljs.main -co '{:target :nodejs :output-to "out/sequences.js" :output-dir "out/sequence-cljs" :optimizations :none :source-map false}' -c suss-oracle.sequences
node out/sequences.js > out/sequence-observations.json
python3 "$sequence_root/scripts/sequence_oracle.py" compare
CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}" \
cargo test --manifest-path "$sequence_root/Cargo.toml" -p suss-cli --test portable_sequences --locked -- --test-threads=2
