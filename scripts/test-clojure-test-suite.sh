#!/bin/sh
# Acceptance command for #211: offline lock/classification/overlap checks, then
# the Suss harness over the fixture and the vendored suite against the reviewed
# oracle references and known-failure baselines. Java/Node are not required.
set -eu
suite_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$suite_root"
python3 scripts/clojure_test_suite.py
python3 scripts/clojure_test_suite.py overlap
python3 -m unittest discover -s scripts -p 'test_clojure_test_suite.py'
CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}" cargo test -p suss-compile --test clojure_test_suite --locked -- --test-threads=1
