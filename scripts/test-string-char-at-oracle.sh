#!/bin/sh
set -eu
char_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
test "$(git -C "$char_root/clojurescript" rev-parse HEAD)" = c4295f303100bbf5afac449242d30bca1126f1a1
python3 -B "$char_root/scripts/oracle_cases.py"
python3 -B "$char_root/scripts/string_char_at_oracle.py" generate
cd "$char_root/tests/oracle"
CLJ_CONFIG=/tmp/suss-oracle-clojure-config CLJ_CACHE=/tmp/suss-oracle-clojure-cache clojure -Srepro -M -m cljs.main -co '{:target :nodejs :output-to "out/string-char-at.js" :output-dir "out/string-char-at-cljs" :optimizations :none :source-map false :cache-analysis false}' -c suss-oracle.string-char-at
node out/string-char-at.js > out/string-char-at-observations.json
python3 -B "$char_root/scripts/string_char_at_oracle.py" compare
