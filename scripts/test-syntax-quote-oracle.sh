#!/bin/sh
# Development-only actual primary execution followed by independently decoded native execution.
set -eu
syntax_quote_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
test "$(git -C "$syntax_quote_root/clojurescript" rev-parse HEAD)" = c4295f303100bbf5afac449242d30bca1126f1a1
python3 "$syntax_quote_root/scripts/core_import.py" --check
python3 "$syntax_quote_root/scripts/reader_core_catalog.py" --check
python3 "$syntax_quote_root/scripts/syntax_quote_oracle.py" generate
cd "$syntax_quote_root/tests/oracle"
CLJ_CONFIG=/tmp/suss-oracle-clojure-config CLJ_CACHE=/tmp/suss-oracle-clojure-cache \
clojure -Srepro -M -m cljs.main -co '{:target :nodejs :output-to "out/syntax-quote.js" :output-dir "out/syntax-quote-cljs" :optimizations :none :source-map false :cache-analysis false :force true}' -c suss-oracle.syntax-quote
node out/syntax-quote.js > out/syntax-quote-observations.json
python3 "$syntax_quote_root/scripts/syntax_quote_oracle.py" compare
CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}" cargo test --manifest-path "$syntax_quote_root/Cargo.toml" -p suss-cli --locked --test compiled_macro_syntax_quote -- --test-threads=2
