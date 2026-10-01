#!/bin/sh
# Development-only reference observations, then independently decoded native tests.
set -eu
object_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
python3 "$object_root/scripts/oracle_cases.py"
python3 "$object_root/scripts/object_method_oracle.py" generate
cd "$object_root/tests/oracle"
CLJ_CONFIG=/tmp/suss-oracle-clojure-config CLJ_CACHE=/tmp/suss-oracle-clojure-cache \
clojure -Srepro -M -m cljs.main -co '{:target :nodejs :output-to "out/object-methods.js" :output-dir "out/object-method-cljs" :optimizations :none :source-map false}' -c suss-oracle.object-methods
node out/object-methods.js > out/object-method-observations.json
python3 "$object_root/scripts/object_method_oracle.py" compare
CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}" \
cargo test --manifest-path "$object_root/Cargo.toml" -p suss-cli --test portable_object_methods --locked -- --test-threads=2
