#!/bin/sh
# Development-only primary observations, then real native compiler graph queries.
set -eu
namespace_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
test "$(git -C "$namespace_root/clojurescript" rev-parse HEAD)" = c4295f303100bbf5afac449242d30bca1126f1a1
cd "$namespace_root/tests/oracle"
mkdir -p out
: > out/namespace-environment-calls.jsonl
CLJ_CONFIG=/tmp/suss-oracle-clojure-config CLJ_CACHE=/tmp/suss-oracle-clojure-cache \
clojure -Srepro -M -m cljs.main -co '{:force true :cache-analysis false :target :nodejs :output-to "out/namespace-environment.js" :output-dir "out/namespace-environment-cljs" :optimizations :none :source-map false}' -c graph.namespace-runner
node out/namespace-environment.js > out/namespace-environment-result.json
python3 "$namespace_root/scripts/namespace_environment_oracle.py"
CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}" cargo test --manifest-path "$namespace_root/Cargo.toml" --locked -p suss-cli --test compiled_macro_analysis_graph native_analysis_graph_namespace_maps_preserve_actual_imports_and_renames_in_both_phases -- --test-threads=2
