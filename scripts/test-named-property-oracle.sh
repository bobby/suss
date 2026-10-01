#!/bin/sh
# Development-only reference observations, then independently decoded native tests.
set -eu
property_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
python3 "$property_root/scripts/oracle_cases.py"
python3 "$property_root/scripts/named_property_oracle.py" generate
cd "$property_root/tests/oracle"
CLJ_CONFIG=/tmp/suss-oracle-clojure-config CLJ_CACHE=/tmp/suss-oracle-clojure-cache \
clojure -Srepro -M -m cljs.main -co '{:target :nodejs :output-to "out/named-properties.js" :output-dir "out/named-property-cljs" :optimizations :none :source-map false}' -c suss-oracle.named-properties
node out/named-properties.js > out/named-property-observations.json
python3 "$property_root/scripts/named_property_oracle.py" compare
CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}" \
cargo test --manifest-path "$property_root/Cargo.toml" -p suss-cli --test portable_named_properties --locked -- --test-threads=2
