#!/bin/sh
# Development-only exact raw analyzer, Node and recorded method facts.
set -eu
method_facts_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
sh "$method_facts_root/scripts/probe-method-recurrence.sh"
python3 "$method_facts_root/scripts/method_recurrence_oracle.py"
