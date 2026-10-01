#!/bin/sh
# Development-only reference observations, then independently decoded native tests.
set -eu
vector_trie_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
python3 "$vector_trie_root/scripts/oracle_cases.py"
python3 "$vector_trie_root/scripts/vector_trie_oracle.py" generate
cd "$vector_trie_root/tests/oracle"
CLJ_CONFIG=/tmp/suss-oracle-clojure-config CLJ_CACHE=/tmp/suss-oracle-clojure-cache \
clojure -Srepro -M -m cljs.main -co '{:target :nodejs :output-to "out/vector-trie.js" :output-dir "out/vector-trie-cljs" :optimizations :none :source-map false}' -c suss-oracle.vector-trie
node out/vector-trie.js > out/vector-trie-observations.json
python3 "$vector_trie_root/scripts/vector_trie_oracle.py" compare
CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}" \
cargo test --manifest-path "$vector_trie_root/Cargo.toml" -p suss-cli --test portable_vector_trie --locked -- --test-threads=2
