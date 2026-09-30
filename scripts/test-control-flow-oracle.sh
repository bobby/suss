#!/bin/sh
# Development-only reference observations, then independently decoded native tests.
set -eu
control_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
python3 "$control_root/scripts/oracle_cases.py"
python3 "$control_root/scripts/control_flow_oracle.py" generate
cd "$control_root/tests/oracle"
CLJ_CONFIG=/tmp/suss-oracle-clojure-config CLJ_CACHE=/tmp/suss-oracle-clojure-cache \
clojure -Srepro -M -m cljs.main -co '{:target :nodejs :output-to "out/control-flow.js" :output-dir "out/control-flow-cljs" :optimizations :none :source-map false}' -c suss-oracle.control-flow
node out/control-flow.js > out/control-flow-observations.json
python3 "$control_root/scripts/control_flow_oracle.py" compare
CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}" \
cargo test --manifest-path "$control_root/Cargo.toml" -p suss-cli --test portable_control_flow --locked -- --test-threads=2
