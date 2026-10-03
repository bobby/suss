#!/bin/sh
# Development-only actual pinned macroexpander observations.
set -eu
form_metadata_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
test "$(git -C "$form_metadata_root/clojurescript" rev-parse HEAD)" = c4295f303100bbf5afac449242d30bca1126f1a1
cd "$form_metadata_root/tests/oracle"
mkdir -p out
CLJ_CONFIG=/tmp/suss-oracle-clojure-config CLJ_CACHE=/tmp/suss-oracle-clojure-cache \
clojure -Srepro -M form-source-metadata-probe.clj > out/form-source-metadata.jsonl
python3 "$form_metadata_root/scripts/form_source_metadata_oracle.py"
