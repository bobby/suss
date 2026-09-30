#!/bin/sh
# Development-only pinned conversions and independently decoded actual GC runtime.
set -eu
numeric_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
numeric_pin=c4295f303100bbf5afac449242d30bca1126f1a1
[ "$(git -C "$numeric_root/clojurescript" rev-parse HEAD)" = "$numeric_pin" ] || { echo 'wrong numeric oracle source pin' >&2; exit 1; }
[ -z "$(git -C "$numeric_root/clojurescript" status --porcelain)" ] || { echo 'dirty numeric oracle source' >&2; exit 1; }
python3 "$numeric_root/scripts/numeric_oracle.py" generate
cd "$numeric_root/tests/oracle"
CLJ_CONFIG=/tmp/suss-oracle-clojure-config CLJ_CACHE=/tmp/suss-oracle-clojure-cache \
clojure -Srepro -M -m cljs.main -co '{:target :nodejs :output-to "out/numeric.js" :output-dir "out/numeric-cljs" :optimizations :none :source-map false}' -c suss-oracle.numeric
node out/numeric.js > out/numeric-observations.json
python3 "$numeric_root/scripts/numeric_oracle.py" compare
CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}" \
cargo test --manifest-path "$numeric_root/Cargo.toml" -p suss-compile --test runtime_abi runtime_abi_numeric_samples --locked -- --test-threads=2
