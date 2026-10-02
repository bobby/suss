#!/bin/sh
# Development-only pinned observations followed by independently decoded Suss.
set -eu
source_apply_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
python3 "$source_apply_root/scripts/oracle_cases.py"
python3 "$source_apply_root/scripts/source_apply_oracle.py" generate
cd "$source_apply_root/tests/oracle"
CLJ_CONFIG=/tmp/suss-oracle-clojure-config CLJ_CACHE=/tmp/suss-oracle-clojure-cache \
clojure -Srepro -M -m cljs.main -co '{:target :nodejs :output-to "out/source-apply.js" :output-dir "out/source-apply-cljs" :optimizations :none :source-map false}' -c suss-oracle.source-apply
node out/source-apply.js > out/source-apply-observations.json
python3 "$source_apply_root/scripts/source_apply_oracle.py" compare
CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}" \
cargo test --manifest-path "$source_apply_root/Cargo.toml" -p suss-cli --test portable_source_apply --locked -- --test-threads=2
