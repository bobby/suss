#!/usr/bin/env python3
"""Exact scalar source AST observations from fresh pinned development analysis."""
import json
from pathlib import Path
from declaration_environment_oracle import parse, validate_data
from oracle_transport import fields, PIN

ROOT = Path(__file__).resolve().parents[1]
LABELS = ["nil", "boolean", "number", "string", "keyword"]


def validate(row):
    if type(row) is not list or len(row) != 4 or row[0] not in LABELS:
        raise ValueError("invalid scalar AST observation identity")
    for key, field in zip(["op", "val", "children"], row[1:]):
        if (type(field) is not list or len(field) != 3 or field[0] != key
                or type(field[1]) is not bool):
            raise ValueError("invalid scalar AST field")
        if not field[1] and field[2] is not None:
            raise ValueError("invalid absent scalar AST field")
        validate_data(field[2])
    if row[1] != ["op", True, ["keyword", ":const"]] or row[2][1] is not True:
        raise ValueError("missing scalar AST operation or value")
    if row[3] != ["children", False, None]:
        raise ValueError("unexpected scalar AST children")
    return row


def compare(expected, actual):
    fields(expected, "schema upstream cases")
    if type(expected["schema"]) is not int or expected["schema"] != 1 or expected["upstream"] != PIN:
        raise ValueError("invalid constant AST corpus identity")
    cases = expected["cases"]
    if type(cases) is not list or type(actual) is not list:
        raise ValueError("invalid constant AST cases")
    cases = [validate(row) for row in cases]
    actual = [validate(row) for row in actual]
    if [row[0] for row in cases] != LABELS or json.dumps(cases) != json.dumps(actual):
        raise ValueError("fresh pinned scalar AST observations differ")


def main():
    expected = parse((ROOT / "tests/oracle/constant-ast-observations.json").read_text())
    observations = [parse(line) for line in
        (ROOT / "tests/oracle/out/constant-ast-calls.jsonl").read_text().splitlines()]
    actual = []
    for row in observations:
        if type(row) is not list or len(row) != 2 or type(row[1]) is not list:
            raise ValueError("malformed fresh scalar AST trace")
        actual.append([row[0], *row[1]])
    compare(expected, actual)
    print("5 fresh pinned scalar AST operation/value/children observations match exactly; native evidence is separate")


if __name__ == "__main__":
    main()
