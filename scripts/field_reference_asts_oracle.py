#!/usr/bin/env python3
"""Exact source-field references from fresh pinned development compilation."""
import json
from pathlib import Path
from declaration_environment_oracle import parse, validate_data
from oracle_transport import fields, PIN

ROOT = Path(__file__).resolve().parents[1]
LABELS = ["plain", "mutable", "unsynchronized", "volatile", "false-mutable", "number-hint", "nil-hint", "false-hint", "shadow"]


def validate(row):
    if type(row) is not list or len(row) != 4 or row[0] not in LABELS:
        raise ValueError("invalid field reference observation")
    for group, keys in zip(row[1:3], [
            ["op", "local", "tag", "children", "val"],
            ["local", "field", "mutable", "unsynchronized-mutable",
             "volatile-mutable", "tag", "shadow", "line", "column"]]):
        if type(group) is not list or len(group) != len(keys):
            raise ValueError("invalid field reference group")
        for key, field in zip(keys, group):
            if (type(field) is not list or len(field) != 3 or field[0] != key
                    or type(field[1]) is not bool):
                raise ValueError("invalid field reference field")
            if not field[1] and field[2] is not None:
                raise ValueError("absent field has a value")
            validate_data(field[2])
    if type(row[3]) is not list or len(row[3]) != 5 or any(type(x) is not bool for x in row[3]):
        raise ValueError("invalid field reference sharing observations")
    return row


def compare(expected, actual):
    fields(expected, "schema upstream cases")
    if type(expected["schema"]) is not int or expected["schema"] != 1 or expected["upstream"] != PIN:
        raise ValueError("invalid reference corpus identity")
    if type(expected["cases"]) is not list or type(actual) is not list:
        raise ValueError("invalid reference corpus cases")
    cases = [validate(row) for row in expected["cases"]]
    actual = [validate(row) for row in actual]
    if [row[0] for row in cases] != LABELS or json.dumps(cases) != json.dumps(actual):
        raise ValueError("fresh pinned field reference observations differ")


def main():
    expected = parse((ROOT / "tests/oracle/field-reference-ast-observations.json").read_text())
    actual = [parse(line) for line in (ROOT / "tests/oracle/out/field-reference-ast-calls.jsonl").read_text().splitlines()]
    compare(expected, actual)
    executed = parse((ROOT / "tests/oracle/out/field-reference-ast-results.json").read_text())
    compare(expected, executed)
    print("9 fresh pinned field reference observations match; all 9 artifact projections executed")


if __name__ == "__main__":
    main()
