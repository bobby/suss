#!/bin/sh
# Development-only reference observations, then independently decoded native tests.
set -eu
indexed_string_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
python3 "$indexed_string_root/scripts/oracle_cases.py"
python3 "$indexed_string_root/scripts/indexed_string_oracle.py" generate
cd "$indexed_string_root/tests/oracle"
CLJ_CONFIG=/tmp/suss-oracle-clojure-config CLJ_CACHE=/tmp/suss-oracle-clojure-cache \
clojure -Srepro -M -m cljs.main -co '{:target :nodejs :output-to "out/indexed-strings.js" :output-dir "out/indexed-string-cljs" :optimizations :none :source-map false}' -c suss-oracle.indexed-strings
node out/indexed-strings.js > out/indexed-string-observations.json
python3 "$indexed_string_root/scripts/indexed_string_oracle.py" compare
CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}" \
cargo test --manifest-path "$indexed_string_root/Cargo.toml" -p suss-cli --test portable_indexed_strings --locked -- --test-threads=2
