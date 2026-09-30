#!/bin/sh
# Development-only pinned source oracle and actual generated ABI fragments.
set -eu
pipeline_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
pipeline_pin=c4295f303100bbf5afac449242d30bca1126f1a1
[ "$(git -C "$pipeline_root/clojurescript" rev-parse HEAD)" = "$pipeline_pin" ] || { echo 'wrong oracle source pin' >&2; exit 1; }
[ -z "$(git -C "$pipeline_root/clojurescript" status --porcelain)" ] || { echo 'dirty oracle source' >&2; exit 1; }
python3 "$pipeline_root/scripts/oracle_cases.py"
python3 "$pipeline_root/scripts/portable_oracle.py" generate
cd "$pipeline_root/tests/oracle"
CLJ_CONFIG=/tmp/suss-oracle-clojure-config CLJ_CACHE=/tmp/suss-oracle-clojure-cache \
clojure -Srepro -M -m cljs.main -co '{:target :nodejs :output-to "out/portable.js" :output-dir "out/portable-cljs" :optimizations :none :source-map false}' -c suss-oracle.portable-pipeline
node out/portable.js > out/portable-observations.json
python3 "$pipeline_root/scripts/portable_oracle.py" compare
CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}" \
cargo test --manifest-path "$pipeline_root/Cargo.toml" -p suss-compile --test portable_pipeline --locked -- --test-threads=2
