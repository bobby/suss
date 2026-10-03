#!/bin/sh
# Development-only pinned observations, then independently decoded native forms.
set -eu
set_data_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
test "$(git -C "$set_data_root/clojurescript" rev-parse HEAD)" = c4295f303100bbf5afac449242d30bca1126f1a1
python3 "$set_data_root/scripts/oracle_cases.py"
python3 "$set_data_root/scripts/set_data_oracle.py" generate
cd "$set_data_root/tests/oracle"
CLJ_CONFIG=/tmp/suss-oracle-clojure-config CLJ_CACHE=/tmp/suss-oracle-clojure-cache \
clojure -Srepro -M -m cljs.main -co '{:target :nodejs :output-to "out/set-data.js" :output-dir "out/set-data-cljs" :optimizations :none :source-map false :cache-analysis false :force true}' -c suss-oracle.set-data
node out/set-data.js > out/set-data-observations.json
python3 "$set_data_root/scripts/set_data_oracle.py" compare
CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}" \
cargo test --manifest-path "$set_data_root/Cargo.toml" -p suss-cli --test compiled_macro_set_oracle --locked -- --test-threads=2
