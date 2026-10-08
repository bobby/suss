#!/bin/sh
# Development-only pinned oracle. Deliberately does not invoke Cargo.
set -eu
sorted_interface_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
python3 "$sorted_interface_root/scripts/cljs_inventory.py" --check
python3 "$sorted_interface_root/scripts/oracle_cases.py"
python3 "$sorted_interface_root/scripts/sorted_interface_oracle.py" generate
cd "$sorted_interface_root/tests/oracle"
CLJ_CONFIG=/tmp/suss-oracle-clojure-config CLJ_CACHE=/tmp/suss-oracle-clojure-cache \
clojure -Srepro -M -m cljs.main -co '{:target :nodejs :output-to "out/sorted-interfaces.js" :output-dir "out/sorted-interface-cljs" :optimizations :none :source-map false}' -c suss-oracle.sorted-interfaces
node out/sorted-interfaces.js > out/sorted-interface-observations.json
python3 "$sorted_interface_root/scripts/sorted_interface_oracle.py" compare
