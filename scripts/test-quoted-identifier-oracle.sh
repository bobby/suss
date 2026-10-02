#!/bin/sh
# Development-only pinned observations followed by independently decoded Suss.
set -eu
quoted_identifier_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
python3 "$quoted_identifier_root/scripts/oracle_cases.py"
python3 "$quoted_identifier_root/scripts/quoted_identifier_oracle.py" generate
cd "$quoted_identifier_root/tests/oracle"
CLJ_CONFIG=/tmp/suss-oracle-clojure-config CLJ_CACHE=/tmp/suss-oracle-clojure-cache \
clojure -Srepro -M -m cljs.main -co '{:target :nodejs :output-to "out/quoted-identifiers.js" :output-dir "out/quoted-identifier-cljs" :optimizations :none :source-map false}' -c suss-oracle.quoted-identifiers
node out/quoted-identifiers.js > out/quoted-identifier-observations.json
python3 "$quoted_identifier_root/scripts/quoted_identifier_oracle.py" compare
CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}" \
cargo test --manifest-path "$quoted_identifier_root/Cargo.toml" -p suss-cli --test portable_quoted_identifiers --locked -- --test-threads=2
