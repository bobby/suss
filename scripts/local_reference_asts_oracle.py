#!/usr/bin/env python3
"""Exact source-local references from fresh pinned development compilation."""
import json
from pathlib import Path
from declaration_environment_oracle import parse, validate_data
from oracle_transport import fields, PIN

ROOT = Path(__file__).resolve().parents[1]
LABELS = ["let", "shadow", "loop", "arg", "rest", "fn"]


def validate(row):
    if type(row) is not list or len(row) != 3 or row[0] not in LABELS:
        raise ValueError("invalid local reference observation")
    if type(row[1]) is not list or len(row[1]) != 4:
        raise ValueError("invalid reference fields")
    for key, field in zip(["op", "local", "arg-id", "variadic?"], row[1]):
        if (type(field) is not list or len(field) != 3 or field[0] != key
                or type(field[1]) is not bool):
            raise ValueError("invalid local reference field")
        if not field[1] and field[2] is not None:
            raise ValueError("absent reference field has a value")
        validate_data(field[2])
    if type(row[2]) is not list or len(row[2]) != 8 or any(type(x) is not bool for x in row[2]):
        raise ValueError("invalid local reference sharing observations")
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
        raise ValueError("fresh pinned local reference observations differ")


def main():
    expected = parse((ROOT / "tests/oracle/local-reference-ast-observations.json").read_text())
    actual = [parse(line) for line in (ROOT / "tests/oracle/out/local-reference-ast-calls.jsonl").read_text().splitlines()]
    compare(expected, actual)
    executed = parse((ROOT / "tests/oracle/out/local-reference-ast-results.json").read_text())
    compare(expected, executed)
    print("6 fresh pinned local reference observations match; all 6 artifact projections executed")


if __name__ == "__main__":
    main()
