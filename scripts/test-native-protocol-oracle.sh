#!/bin/sh
# Development-only pinned observations followed by independently decoded Suss.
set -eu
native_protocol_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
python3 "$native_protocol_root/scripts/oracle_cases.py"
python3 "$native_protocol_root/scripts/native_protocol_oracle.py" generate
cd "$native_protocol_root/tests/oracle"
CLJ_CONFIG=/tmp/suss-oracle-clojure-config CLJ_CACHE=/tmp/suss-oracle-clojure-cache \
clojure -Srepro -M -m cljs.main -co '{:target :nodejs :output-to "out/native-protocols.js" :output-dir "out/native-protocol-cljs" :optimizations :none :source-map false}' -c suss-oracle.native-protocols
node out/native-protocols.js > out/native-protocol-observations.json
python3 "$native_protocol_root/scripts/native_protocol_oracle.py" compare
CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}" \
cargo test --manifest-path "$native_protocol_root/Cargo.toml" -p suss-cli --test portable_native_protocols --locked -- --test-threads=2
