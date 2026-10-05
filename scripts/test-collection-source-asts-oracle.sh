#!/bin/sh
# Development-only exact analyzer and executed artifact evidence.
set -eu
collection_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
sh "$collection_root/scripts/probe-collection-source-asts.sh"
python3 "$collection_root/scripts/collection_source_asts_oracle.py"
