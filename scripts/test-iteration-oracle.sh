#!/bin/sh
# Development-only reference observations, then independently decoded native tests.
set -eu
iteration_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
python3 "$iteration_root/scripts/oracle_cases.py"
python3 "$iteration_root/scripts/iteration_oracle.py" generate
cd "$iteration_root/tests/oracle"
CLJ_CONFIG=/tmp/suss-oracle-clojure-config CLJ_CACHE=/tmp/suss-oracle-clojure-cache \
clojure -Srepro -M -m cljs.main -co '{:target :nodejs :output-to "out/iterations.js" :output-dir "out/iteration-cljs" :optimizations :none :source-map false}' -c suss-oracle.iterations
node out/iterations.js > out/iteration-observations.json
python3 "$iteration_root/scripts/iteration_oracle.py" compare
# Separate exact pinned observation of the explicitly unsupported named length write.
# Native tests require a typed error here; this is not a shared passing value case.
python3 "$iteration_root/scripts/iteration_oracle.py" generate boundary
CLJ_CONFIG=/tmp/suss-oracle-clojure-config CLJ_CACHE=/tmp/suss-oracle-clojure-cache \
clojure -Srepro -M -m cljs.main -co '{:target :nodejs :output-to "out/iterations.js" :output-dir "out/iteration-cljs" :optimizations :none :source-map false}' -c suss-oracle.iterations
node out/iterations.js > out/iteration-length-boundary-observations.json
python3 "$iteration_root/scripts/iteration_oracle.py" compare boundary
CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}" \
cargo test --manifest-path "$iteration_root/Cargo.toml" -p suss-cli --test portable_iteration --locked -- --test-threads=2
