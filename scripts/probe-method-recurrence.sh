#!/bin/sh
# Development-only candidate probe. Inspect fresh outputs before freezing a corpus.
set -eu
method_recurrence_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
test "$(git -C "$method_recurrence_root/clojurescript" rev-parse HEAD)" = c4295f303100bbf5afac449242d30bca1126f1a1
cd "$method_recurrence_root/tests/oracle"
mkdir -p out
: > out/method-recurrence-calls.jsonl
CLJ_CONFIG=/tmp/suss-oracle-clojure-config CLJ_CACHE=/tmp/suss-oracle-clojure-cache \
clojure -Srepro -M -m cljs.main -co '{:force true :cache-analysis false :target :nodejs :output-to "out/method-recurrences.js" :output-dir "out/method-recurrences-cljs" :optimizations :none :source-map false}' -c suss-oracle.method-recurrence-runner
node out/method-recurrences.js > out/method-recurrence-results.json
