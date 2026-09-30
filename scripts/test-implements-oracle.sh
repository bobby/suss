#!/bin/sh
# Development-only reference observations, then independently decoded native tests.
set -eu
implements_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
python3 "$implements_root/scripts/oracle_cases.py"
python3 "$implements_root/scripts/implements_oracle.py" generate
cd "$implements_root/tests/oracle"
CLJ_CONFIG=/tmp/suss-oracle-clojure-config CLJ_CACHE=/tmp/suss-oracle-clojure-cache \
clojure -Srepro -M -m cljs.main -co '{:target :nodejs :output-to "out/implements.js" :output-dir "out/implements-cljs" :optimizations :none :source-map false}' -c suss-oracle.implements
node out/implements.js > out/implements-observations.json
python3 "$implements_root/scripts/implements_oracle.py" compare
CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}" \
cargo test --manifest-path "$implements_root/Cargo.toml" -p suss-cli --test portable_implements --locked -- --test-threads=2
