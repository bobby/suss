#!/bin/sh
# Development-only exact raw analyzer and executed Node evidence.
set -eu
control_ast_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
sh "$control_ast_root/scripts/probe-control-source-asts.sh"
python3 "$control_ast_root/scripts/control_source_asts_oracle.py"
