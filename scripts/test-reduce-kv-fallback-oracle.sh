#!/bin/sh
# Dedicated #244 reference and native test lane; intentionally no broad reduction suite.
set -eu
reduce_kv_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
reduce_kv_pin=c4295f303100bbf5afac449242d30bca1126f1a1
if [ "$(git -C "$reduce_kv_root/clojurescript" rev-parse HEAD)" != "$reduce_kv_pin" ]; then
  echo "reduce-kv oracle requires the exact pinned ClojureScript checkout" >&2
  exit 1
fi
if [ -n "$(git -C "$reduce_kv_root/clojurescript" status --porcelain)" ]; then
  echo "reduce-kv oracle source checkout must be clean" >&2
  exit 1
fi
python3 "$reduce_kv_root/scripts/reduce_kv_fallback_oracle.py" generate
cd "$reduce_kv_root/tests/oracle"
CLJ_CONFIG=/tmp/suss-oracle-clojure-config CLJ_CACHE=/tmp/suss-oracle-clojure-cache \
clojure -Srepro -M -m cljs.main -co '{:target :nodejs :output-to "out/reduce-kv-fallback.js" :output-dir "out/reduce-kv-fallback-cljs" :optimizations :none :source-map false :cache-analysis false :force true}' -c suss-oracle.reduce-kv-fallback
node out/reduce-kv-fallback.js > out/reduce-kv-fallback-observations.json
python3 "$reduce_kv_root/scripts/reduce_kv_fallback_oracle.py" compare
CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}" \
cargo test --manifest-path "$reduce_kv_root/Cargo.toml" -p suss-cli --test portable_reduce_kv_fallback --locked -- --test-threads=2
