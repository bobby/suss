#!/bin/sh
set -eu
constructor_ast_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
sh "$constructor_ast_root/scripts/probe-constructor-source-asts.sh"
python3 "$constructor_ast_root/scripts/constructor_source_asts_oracle.py"
