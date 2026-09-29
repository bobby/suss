#!/bin/sh
# Development-only JVM compiler + Node oracle, never a shipped Suss dependency.
set -eu
oracle_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
oracle_pin=c4295f303100bbf5afac449242d30bca1126f1a1
[ "$(git -C "$oracle_root/clojurescript" rev-parse HEAD)" = "$oracle_pin" ] || { echo 'wrong oracle source pin' >&2; exit 1; }
[ -z "$(git -C "$oracle_root/clojurescript" status --porcelain)" ] || { echo 'dirty oracle source' >&2; exit 1; }
python3 "$oracle_root/scripts/oracle_cases.py"
cd "$oracle_root/tests/oracle"
CLJ_CONFIG=/tmp/suss-oracle-clojure-config CLJ_CACHE=/tmp/suss-oracle-clojure-cache \
clojure -Srepro -M -m cljs.main -co '{:target :nodejs :output-to "out/oracle.js" :output-dir "out/cljs" :optimizations :none :source-map false}' -c suss-oracle.main
node out/oracle.js > out/observations.json
python3 "$oracle_root/scripts/oracle_transport.py" out/observations.json
SUSS_ORACLE_REFERENCE="$oracle_root/tests/oracle/out/observations.json" \
CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}" \
cargo test --manifest-path "$oracle_root/Cargo.toml" -p suss-compile --test oracle --locked -- --test-threads=2
