#!/bin/sh
# Development-only method compiler source roles.
set -eu
role_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$role_root/tests/oracle"
CLJ_CONFIG=/tmp/suss-oracle-clojure-config CLJ_CACHE=/tmp/suss-oracle-clojure-cache \
clojure -Srepro -M -m cljs.main -co '{:force true :cache-analysis false :target :nodejs :output-to "out/method-role.js" :output-dir "out/method-role-cljs" :optimizations :none :source-map false}' -c suss-oracle.method-role-facts
node out/method-role.js > out/method-role-results.json
python3 "$role_root/scripts/method_role_oracle.py"
CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}" cargo test --manifest-path "$role_root/Cargo.toml" -p suss-cli --locked --test compiled_macro_method_roles -- --test-threads=2
