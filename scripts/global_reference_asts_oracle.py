#!/usr/bin/env python3
"""Exact global AST facts from fresh pinned development compilation/execution."""
import json
from pathlib import Path
from declaration_environment_oracle import parse, validate_data
from oracle_transport import fields, PIN

ROOT = Path(__file__).resolve().parents[1]
LABELS = ["scalar", "qualified", "hint", "dynamic", "function", "declared",
          "raw-metadata", "core", "core-alias", "revision-before", "revision-after"]
KEYS = [["op", "name", "ns", "tag"],
        ["op", "name", "ns", "tag", "doc", "declared", "dynamic", "fn-var", "ret-tag"],
        ["op", "ns"]]


def validate(row):
    if type(row) is not list or len(row) != 5 or row[0] not in LABELS:
        raise ValueError("invalid global reference observation")
    for record, keys in zip(row[1:4], KEYS):
        if type(record) is not list or len(record) != len(keys):
            raise ValueError("invalid global reference fields")
        for key, field in zip(keys, record):
            if (type(field) is not list or len(field) != 3 or field[0] != key
                    or type(field[1]) is not bool):
                raise ValueError("invalid global reference field")
            if not field[1] and field[2] is not None:
                raise ValueError("absent global reference field has a value")
            validate_data(field[2])
    if type(row[4]) is not list or len(row[4]) != 4 or any(type(x) is not bool for x in row[4]):
        raise ValueError("invalid global reference agreement observations")
    return row


def compare(expected, actual):
    fields(expected, "schema upstream cases")
    if type(expected["schema"]) is not int or expected["schema"] != 1 or expected["upstream"] != PIN:
        raise ValueError("invalid global reference corpus identity")
    if type(expected["cases"]) is not list or type(actual) is not list:
        raise ValueError("invalid global reference corpus cases")
    cases = [validate(row) for row in expected["cases"]]
    actual = [validate(row) for row in actual]
    if [row[0] for row in cases] != LABELS or json.dumps(cases) != json.dumps(actual):
        raise ValueError("fresh pinned global reference observations differ")


def main():
    expected = parse((ROOT / "tests/oracle/global-reference-ast-observations.json").read_text())
    actual = [parse(line) for line in (ROOT / "tests/oracle/out/global-reference-ast-calls.jsonl").read_text().splitlines()]
    compare(expected, actual)
    executed = parse((ROOT / "tests/oracle/out/global-reference-ast-results.json").read_text())
    if type(executed) is not list or len(executed) != 3:
        raise ValueError("invalid executed global reference report")
    compare(expected, executed[0])
    if executed[1] is not False or type(executed[2]) not in (int, float) or executed[2] != 1:
        raise ValueError("global inspection replayed initialization or lost the live redefinition")
    print("11 fresh pinned global reference observations match; all 11 artifact projections executed")


if __name__ == "__main__":
    main()
