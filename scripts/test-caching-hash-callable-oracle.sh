#!/bin/sh
# Development-only: pinned callable hashing, then both compiled caller phases.
set -eu
hash_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
test "$(git -C "$hash_root/clojurescript" rev-parse HEAD)" = c4295f303100bbf5afac449242d30bca1126f1a1
test -z "$(git -C "$hash_root/clojurescript" status --porcelain)"
python3 "$hash_root/scripts/oracle_cases.py"
python3 "$hash_root/scripts/caching_hash_oracle.py" generate callable
cd "$hash_root/tests/oracle"
CLJ_CONFIG=/tmp/suss-oracle-clojure-config CLJ_CACHE=/tmp/suss-oracle-clojure-cache \
clojure -J-Xmx1g -J-XX:ActiveProcessorCount=2 -Srepro -M -m cljs.main -co '{:force true :cache-analysis false :target :nodejs :output-to "out/caching-hash-callable.js" :output-dir "out/caching-hash-callable-cljs" :optimizations :none :source-map false}' -c suss-oracle.caching-hash
node out/caching-hash-callable.js > out/caching-hash-callable-observations.json
python3 "$hash_root/scripts/caching_hash_oracle.py" compare callable
CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}" \
cargo test --manifest-path "$hash_root/Cargo.toml" --workspace --locked --test portable_caching_hash caching_hash_invokes_callable_objects_and_skips_hit_effects_in_both_phases -- --test-threads=2
