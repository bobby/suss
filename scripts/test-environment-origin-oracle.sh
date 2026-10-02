#!/bin/sh
# Development-only observations of pinned macro source-position facts.
set -eu
origin_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$origin_root/tests/oracle"
CLJ_CONFIG=/tmp/suss-oracle-clojure-config CLJ_CACHE=/tmp/suss-oracle-clojure-cache \
clojure -Srepro -M -m cljs.main -co '{:target :nodejs :output-to "out/environment-origin.js" :output-dir "out/environment-origin-cljs" :optimizations :none :source-map false}' -c suss-oracle.environment-origin
node out/environment-origin.js > out/environment-origin-observations.json
CLJ_CONFIG=/tmp/suss-oracle-clojure-config CLJ_CACHE=/tmp/suss-oracle-clojure-cache \
clojure -Srepro -M -m cljs.main -co '{:target :nodejs :output-to "out/metadata-position.js" :output-dir "out/metadata-position-cljs" :optimizations :none :source-map false}' -c suss-oracle.metadata-position
node out/metadata-position.js > out/metadata-position-observations.json
python3 "$origin_root/scripts/environment_origin_oracle.py"
CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}" cargo test --manifest-path "$origin_root/Cargo.toml" -p suss-cli --locked --test compiled_macro_source_positions -- --test-threads=2
