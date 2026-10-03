#!/bin/sh
# Development-only upstream evidence; no native &env acceptance is claimed.
set -eu
rich_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
rich_pin=$(git -C "$rich_root/clojurescript" rev-parse HEAD)
if [ "$rich_pin" != c4295f303100bbf5afac449242d30bca1126f1a1 ]; then
  echo "Rich environment oracle requires the pinned local ClojureScript checkout" >&2
  exit 1
fi
cd "$rich_root/tests/oracle"
CLJ_CONFIG=/tmp/suss-oracle-clojure-config CLJ_CACHE=/tmp/suss-oracle-clojure-cache \
clojure -Srepro -M -m cljs.main -co '{:force true :cache-analysis false :target :nodejs :output-to "out/rich-environment.js" :output-dir "out/rich-environment-cljs" :optimizations :none :source-map false}' -c suss-oracle.rich-environment-facts
node out/rich-environment.js > out/rich-environment-results.json
python3 "$rich_root/scripts/rich_environment_oracle.py"
