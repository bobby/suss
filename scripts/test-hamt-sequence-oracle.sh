#!/bin/sh
# Development-only pinned observations, then independently decoded native forms.
set -eu
hamt_sequence_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
python3 "$hamt_sequence_root/scripts/oracle_cases.py"
python3 "$hamt_sequence_root/scripts/hamt_sequence_oracle.py" generate
cd "$hamt_sequence_root/tests/oracle"
CLJ_CONFIG=/tmp/suss-oracle-clojure-config CLJ_CACHE=/tmp/suss-oracle-clojure-cache \
clojure -Srepro -M -m cljs.main -co '{:target :nodejs :output-to "out/hamt-sequence.js" :output-dir "out/hamt-sequence-cljs" :optimizations :none :source-map false}' -c suss-oracle.hamt-sequence
node out/hamt-sequence.js > out/hamt-sequence-observations.json
python3 "$hamt_sequence_root/scripts/hamt_sequence_oracle.py" compare
CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}" \
cargo test --manifest-path "$hamt_sequence_root/Cargo.toml" -p suss-cli --test compiled_macro_hamt_sequences --locked -- --test-threads=2
