#!/bin/sh
# Development-only pinned ClojureScript execution and native acceptance.
set -eu
realized_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
python3 "$realized_root/scripts/core_import.py" --check
python3 "$realized_root/scripts/realized_oracle.py" generate
cd "$realized_root/tests/oracle"
CLJ_CONFIG=/tmp/suss-oracle-clojure-config CLJ_CACHE=/tmp/suss-oracle-clojure-cache \
clojure -Srepro -M -m cljs.main -co '{:target :nodejs :output-to "out/realized.js" :output-dir "out/realized-cljs" :optimizations :none :source-map false :cache-analysis false :force true}' -c suss-oracle.realized
node out/realized.js > out/realized-observations.json
python3 "$realized_root/scripts/realized_oracle.py" compare
CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}" \
cargo test --manifest-path "$realized_root/Cargo.toml" -p suss-cli --locked --test portable_realized -- --test-threads=2
