#!/bin/sh
set -eu
helper_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
test "$(git -C "$helper_root/clojurescript" rev-parse HEAD)" = c4295f303100bbf5afac449242d30bca1126f1a1
python3 -B "$helper_root/scripts/oracle_cases.py"
python3 -B "$helper_root/scripts/function_parameter_oracle.py" generate
cd "$helper_root/tests/oracle"
CLJ_CONFIG=/tmp/suss-oracle-clojure-config CLJ_CACHE=/tmp/suss-oracle-clojure-cache clojure -Srepro -M -m cljs.main -co '{:target :nodejs :output-to "out/function-parameter.js" :output-dir "out/function-parameter-cljs" :optimizations :none :source-map false :cache-analysis false}' -c suss-oracle.function-parameter
node out/function-parameter.js > out/function-parameter-observations.json
python3 -B "$helper_root/scripts/function_parameter_oracle.py" compare
