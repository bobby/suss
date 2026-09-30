#!/bin/sh
# Development-only pinned observations followed by independently decoded Suss.
set -eu
mutable_field_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
python3 "$mutable_field_root/scripts/oracle_cases.py"
python3 "$mutable_field_root/scripts/mutable_field_oracle.py" generate
cd "$mutable_field_root/tests/oracle"
CLJ_CONFIG=/tmp/suss-oracle-clojure-config CLJ_CACHE=/tmp/suss-oracle-clojure-cache \
clojure -Srepro -M -m cljs.main -co '{:target :nodejs :output-to "out/mutable-fields.js" :output-dir "out/mutable-field-cljs" :optimizations :none :source-map false}' -c suss-oracle.mutable-fields
node out/mutable-fields.js > out/mutable-field-observations.json
python3 "$mutable_field_root/scripts/mutable_field_oracle.py" compare
CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}" \
cargo test --manifest-path "$mutable_field_root/Cargo.toml" -p suss-cli --test portable_mutable_fields --locked -- --test-threads=2
