#!/bin/sh
# Development-only reference observations, then independently decoded native tests.
set -eu
case_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
python3 "$case_root/scripts/oracle_cases.py"
python3 "$case_root/scripts/case_oracle.py" generate
python3 "$case_root/scripts/case_oracle.py" generate-boundary
cd "$case_root/tests/oracle"
CLJ_CONFIG=/tmp/suss-oracle-clojure-config CLJ_CACHE=/tmp/suss-oracle-clojure-cache \
clojure -Srepro -M -m cljs.main -co '{:target :nodejs :output-to "out/case.js" :output-dir "out/case-cljs" :optimizations :none :source-map false}' -c suss-oracle.case
node out/case.js > out/case-observations.json
python3 "$case_root/scripts/case_oracle.py" compare
CLJ_CONFIG=/tmp/suss-oracle-clojure-config CLJ_CACHE=/tmp/suss-oracle-clojure-cache \
clojure -Srepro -M -m cljs.main -co '{:target :nodejs :output-to "out/case-reference-boundary.js" :output-dir "out/case-boundary-cljs" :optimizations :none :source-map false}' -c suss-oracle.case-reference-boundary
python3 "$case_root/scripts/case_oracle.py" compare-boundary
CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}" \
cargo test --manifest-path "$case_root/Cargo.toml" -p suss-cli --test portable_case --locked -- --test-threads=2
