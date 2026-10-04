#!/usr/bin/env python3
"""Exact quoted-source AST observations; a candidate is not execution evidence."""
import json
from pathlib import Path
from declaration_environment_oracle import parse, validate_data
from oracle_transport import fields, PIN

ROOT = Path(__file__).resolve().parents[1]
LABELS = ['nil', 'boolean', 'number', 'string', 'keyword', 'symbol', 'qualified', 'empty-list', 'list', 'vector', 'map', 'set', 'metadata', 'effect-data']
KEYS = ["op", "literal?", "form", "tag", "children", "val"]
EXPR_KEYS = ["op", "literal?", "form", "tag", "val", "children"]


def validate(row):
    if type(row) is not list or len(row) != 4 or row[0] not in LABELS:
        raise ValueError("invalid quote observation")
    for record, keys in zip(row[1:3], [KEYS, EXPR_KEYS]):
        if type(record) is not list or len(record) != len(keys):
            raise ValueError("invalid quote fields")
        for key, field in zip(keys, record):
            if (type(field) is not list or len(field) != 3 or field[0] != key
                    or type(field[1]) is not bool):
                raise ValueError("invalid quote field")
            if not field[1] and field[2] is not None:
                raise ValueError("absent quote field has a value")
            validate_data(field[2])
    if type(row[3]) is not list or len(row[3]) != 7 or any(type(x) is not bool for x in row[3][:6]):
        raise ValueError("invalid quote agreements")
    validate_data(row[3][6])
    return row


def compare(expected, actual):
    fields(expected, "schema upstream cases")
    if type(expected["schema"]) is not int or expected["schema"] != 1 or expected["upstream"] != PIN:
        raise ValueError("invalid quote corpus identity")
    if type(expected["cases"]) is not list or type(actual) is not list:
        raise ValueError("invalid quote corpus cases")
    cases = [validate(row) for row in expected["cases"]]
    actual = [validate(row) for row in actual]
    if [row[0] for row in cases] != LABELS or json.dumps(cases) != json.dumps(actual):
        raise ValueError("fresh pinned quote observations differ")


def main():
    expected = parse((ROOT / "tests/oracle/quote-ast-observations.json").read_text())
    actual = [parse(line) for line in (ROOT / "tests/oracle/out/quote-ast-calls.jsonl").read_text().splitlines()]
    compare(expected, actual)
    executed = parse((ROOT / "tests/oracle/out/quote-ast-results.json").read_text())
    if type(executed) is not list or len(executed) != 2:
        raise ValueError("invalid executed quote report")
    compare(expected, executed[0])
    if type(executed[1]) not in (int, float) or executed[1] != 0:
        raise ValueError("quoted source was executed during inspection")
    print("14 fresh pinned quote AST observations match; all 14 artifact projections executed")


if __name__ == "__main__":
    main()
