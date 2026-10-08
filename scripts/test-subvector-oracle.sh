#!/bin/sh
# Development-only pinned oracle; native execution is a separate gate.
set -eu
subvector_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
subvector_pin=c4295f303100bbf5afac449242d30bca1126f1a1
if [ "$(git -C "$subvector_root/clojurescript" rev-parse HEAD)" != "$subvector_pin" ]; then
  echo "subvector oracle requires the exact pinned ClojureScript checkout" >&2
  exit 1
fi
python3 "$subvector_root/scripts/oracle_cases.py"
python3 "$subvector_root/scripts/subvector_oracle.py" generate
cd "$subvector_root/tests/oracle"
CLJ_CONFIG=/tmp/suss-oracle-clojure-config CLJ_CACHE=/tmp/suss-oracle-clojure-cache \
clojure -Srepro -M -m cljs.main -co '{:target :nodejs :output-to "out/subvector.js" :output-dir "out/subvector-cljs" :optimizations :none :source-map false :cache-analysis false}' -c suss-oracle.subvector
node out/subvector.js > out/subvector-observations.json
python3 "$subvector_root/scripts/subvector_oracle.py" compare
