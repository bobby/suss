#!/bin/sh
set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
test "$(git -C "$root/clojurescript" rev-parse HEAD)" = c4295f303100bbf5afac449242d30bca1126f1a1
python3 -B "$root/scripts/error_constructor_oracle.py" check
cd "$root/tests/oracle"
CLJ_CONFIG=/tmp/suss-oracle-clojure-config CLJ_CACHE=/tmp/suss-oracle-clojure-cache clojure -Srepro -M -m cljs.main -co '{:target :nodejs :output-to "out/error-constructors.js" :output-dir "out/error-constructors-cljs" :optimizations :none :source-map false :cache-analysis false}' -c suss-oracle.error-constructors
node out/error-constructors.js > out/error-constructors.json
python3 -B "$root/scripts/error_constructor_oracle.py" compare
