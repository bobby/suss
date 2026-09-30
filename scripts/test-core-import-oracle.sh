#!/bin/sh
# Development-only pinned observations, then actual imported Suss execution.
set -eu
core_import_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
python3 "$core_import_root/scripts/core_import.py" --check
python3 "$core_import_root/scripts/oracle_cases.py"
python3 "$core_import_root/scripts/core_import_oracle.py" generate
cd "$core_import_root/tests/oracle"
CLJ_CONFIG=/tmp/suss-oracle-clojure-config CLJ_CACHE=/tmp/suss-oracle-clojure-cache \
clojure -Srepro -M -m cljs.main -co '{:target :nodejs :output-to "out/core-import.js" :output-dir "out/core-import-cljs" :optimizations :none :source-map false}' -c suss-oracle.core-import
node out/core-import.js > out/core-import-observations.json
python3 "$core_import_root/scripts/core_import_oracle.py" compare
CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}" \
cargo test --manifest-path "$core_import_root/Cargo.toml" -p suss-cli --test core_import --locked -- --test-threads=2
