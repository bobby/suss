#!/bin/sh
# Development-only exact macro snapshot/catalog observations; no shipped JVM.
set -eu
export_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
test "$(git -C "$export_root/clojurescript" rev-parse HEAD)" = c4295f303100bbf5afac449242d30bca1126f1a1
cd "$export_root/tests/oracle"
mkdir -p out
CLJ_CONFIG=/tmp/suss-oracle-clojure-config CLJ_CACHE=/tmp/suss-oracle-clojure-cache \
clojure -Srepro -M export-metadata-probe.clj > out/export-metadata-observations.edn
cat out/export-metadata-observations.edn
