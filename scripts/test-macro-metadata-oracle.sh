#!/bin/sh
# Development-only pinned observations followed by independently decoded Suss.
set -eu
macro_metadata_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
python3 "$macro_metadata_root/scripts/oracle_cases.py"
python3 "$macro_metadata_root/scripts/macro_metadata_oracle.py" generate
cd "$macro_metadata_root/tests/oracle"
CLJ_CONFIG=/tmp/suss-oracle-clojure-config CLJ_CACHE=/tmp/suss-oracle-clojure-cache \
clojure -Srepro -M -m cljs.main -co '{:target :nodejs :output-to "out/macro-metadata.js" :output-dir "out/macro-metadata-cljs" :optimizations :none :source-map false}' -c suss-oracle.macro-metadata
node out/macro-metadata.js > out/macro-metadata-observations.json
python3 "$macro_metadata_root/scripts/macro_metadata_oracle.py" compare
CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}" \
cargo test --manifest-path "$macro_metadata_root/Cargo.toml" -p suss-cli --test compiled_macro_metadata --locked -- --test-threads=2
