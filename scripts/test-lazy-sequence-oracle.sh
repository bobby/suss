#!/bin/sh
# Development-only actual primary execution followed by native checks.
set -eu
lazy_sequence_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
test "$(git -C "$lazy_sequence_root/clojurescript" rev-parse HEAD)" = c4295f303100bbf5afac449242d30bca1126f1a1
python3 "$lazy_sequence_root/scripts/core_import.py" --check
python3 "$lazy_sequence_root/scripts/lazy_sequence_oracle.py" generate
cd "$lazy_sequence_root/tests/oracle"
CLJ_CONFIG=/tmp/suss-oracle-clojure-config CLJ_CACHE=/tmp/suss-oracle-clojure-cache \
clojure -Srepro -M -m cljs.main -co '{:target :nodejs :output-to "out/lazy-sequences.js" :output-dir "out/lazy-sequences-cljs" :optimizations :none :source-map false :cache-analysis false :force true}' -c suss-oracle.lazy-sequences
node out/lazy-sequences.js > out/lazy-sequence-observations.json
python3 "$lazy_sequence_root/scripts/lazy_sequence_oracle.py" compare
CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}" cargo test --manifest-path "$lazy_sequence_root/Cargo.toml" -p suss-cli --locked --test portable_lazy_sequences -- --test-threads=2
