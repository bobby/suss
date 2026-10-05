#!/bin/sh
# Development-only exact raw analyzer, Node and recorded name binding facts.
set -eu
function_name_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
sh "$function_name_root/scripts/probe-function-name-asts.sh"
python3 "$function_name_root/scripts/function_name_ast_oracle.py"
