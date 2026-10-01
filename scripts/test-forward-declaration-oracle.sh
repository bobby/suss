#!/bin/sh
# Development-only reference observations, then independently decoded native tests.
set -eu
declaration_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
python3 "$declaration_root/scripts/oracle_cases.py"
python3 "$declaration_root/scripts/forward_declaration_oracle.py" generate
cd "$declaration_root/tests/oracle"
CLJ_CONFIG=/tmp/suss-oracle-clojure-config CLJ_CACHE=/tmp/suss-oracle-clojure-cache \
clojure -Srepro -M -m cljs.main -co '{:target :nodejs :output-to "out/forward-declarations.js" :output-dir "out/forward-declaration-cljs" :optimizations :none :source-map false}' -c suss-oracle.forward-declarations
node out/forward-declarations.js > out/forward-declaration-observations.json
python3 "$declaration_root/scripts/forward_declaration_oracle.py" compare
CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}" \
cargo test --manifest-path "$declaration_root/Cargo.toml" -p suss-cli --test portable_forward_declarations --locked -- --test-threads=2
