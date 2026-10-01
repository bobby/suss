#!/bin/sh
# Development-only reference observations, then independently decoded native tests.
set -eu
type_scope_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
python3 "$type_scope_root/scripts/oracle_cases.py"
python3 "$type_scope_root/scripts/type_method_scope_oracle.py" generate
cd "$type_scope_root/tests/oracle"
CLJ_CONFIG=/tmp/suss-oracle-clojure-config CLJ_CACHE=/tmp/suss-oracle-clojure-cache \
clojure -Srepro -M -m cljs.main -co '{:target :nodejs :output-to "out/type-method-scopes.js" :output-dir "out/type-method-scope-cljs" :optimizations :none :source-map false}' -c suss-oracle.type-method-scopes
node out/type-method-scopes.js > out/type-method-scope-observations.json
python3 "$type_scope_root/scripts/type_method_scope_oracle.py" compare
CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}" \
cargo test --manifest-path "$type_scope_root/Cargo.toml" -p suss-cli --test portable_type_method_scopes --locked -- --test-threads=2
