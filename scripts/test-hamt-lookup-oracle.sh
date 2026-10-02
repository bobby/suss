#!/bin/sh
# Development-only pinned observations, then independently decoded native forms.
set -eu
hamt_lookup_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
python3 "$hamt_lookup_root/scripts/oracle_cases.py"
python3 "$hamt_lookup_root/scripts/hamt_lookup_oracle.py" generate
cd "$hamt_lookup_root/tests/oracle"
CLJ_CONFIG=/tmp/suss-oracle-clojure-config CLJ_CACHE=/tmp/suss-oracle-clojure-cache \
clojure -Srepro -M -m cljs.main -co '{:target :nodejs :output-to "out/hamt-lookup.js" :output-dir "out/hamt-lookup-cljs" :optimizations :none :source-map false}' -c suss-oracle.hamt-lookup
node out/hamt-lookup.js > out/hamt-lookup-observations.json
python3 "$hamt_lookup_root/scripts/hamt_lookup_oracle.py" compare
CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}" \
cargo test --manifest-path "$hamt_lookup_root/Cargo.toml" -p suss-cli --test compiled_macro_hamt_lookups --locked -- --test-threads=2
