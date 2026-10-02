#!/bin/sh
# Development-only compiler environment observations.
set -eu
context_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$context_root/tests/oracle"
CLJ_CONFIG=/tmp/suss-oracle-clojure-config CLJ_CACHE=/tmp/suss-oracle-clojure-cache \
clojure -Srepro -M -m cljs.main -co '{:force true :cache-analysis false :target :nodejs :output-to "out/context-facts.js" :output-dir "out/context-facts-cljs" :optimizations :none :source-map false}' -c suss-oracle.context-facts
node out/context-facts.js > out/context-facts-results.json
python3 "$context_root/scripts/context_facts_oracle.py"
CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}" cargo test --manifest-path "$context_root/Cargo.toml" -p suss-cli --locked --test compiled_macro_analysis_context -- --test-threads=2
