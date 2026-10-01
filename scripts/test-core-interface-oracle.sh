#!/bin/sh
# Development-only reference observations, then independently decoded native tests.
set -eu
interface_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
python3 "$interface_root/scripts/oracle_cases.py"
python3 "$interface_root/scripts/core_interface_oracle.py" generate
cd "$interface_root/tests/oracle"
CLJ_CONFIG=/tmp/suss-oracle-clojure-config CLJ_CACHE=/tmp/suss-oracle-clojure-cache \
clojure -Srepro -M -m cljs.main -co '{:target :nodejs :output-to "out/core-interfaces.js" :output-dir "out/core-interface-cljs" :optimizations :none :source-map false}' -c suss-oracle.core-interfaces
node out/core-interfaces.js > out/core-interface-observations.json
python3 "$interface_root/scripts/core_interface_oracle.py" compare
CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}" \
cargo test --manifest-path "$interface_root/Cargo.toml" -p suss-cli --test core_sequence_interfaces --locked -- --test-threads=2
