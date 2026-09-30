#!/bin/sh
# Bounded bootstrap provenance/import gate; not the full M4/M7 exit gate.
set -eu
core_import_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
python3 "$core_import_root/scripts/cljs_inventory.py" --check
python3 "$core_import_root/scripts/cljs_reviews.py"
python3 "$core_import_root/scripts/core_import.py" --check
CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}" \
cargo test --manifest-path "$core_import_root/Cargo.toml" -p suss-cli --test core_import --locked -- --test-threads=2
