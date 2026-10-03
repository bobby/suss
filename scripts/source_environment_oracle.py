#!/usr/bin/env python3
"""Exact primary source-macro projections; full portable AST remains pending."""
from pathlib import Path
import json
from declaration_environment_oracle import parse, validate_data
from oracle_transport import fields, PIN
ROOT = Path(__file__).resolve().parents[1]
LABELS = ['lexical', 'function', 'variadic']
WIDTHS = [7, 3, 4]
RESULT = [7, 1, 3, 4]


def validate(row):
    if type(row) is not list or len(row) != 2 or row[0] not in LABELS:
        raise ValueError('invalid source environment identity')
    validate_data(row[1])
    if (type(row[1]) is not list or row[1][0] != 'vector'
            or len(row[1][1]) != WIDTHS[LABELS.index(row[0])]):
        raise ValueError('invalid source environment projection')
    return row


def compare(expected, actual, result):
    fields(expected, 'schema upstream cases result')
    if type(expected['schema']) is not int or expected['schema'] != 1 or expected['upstream'] != PIN:
        raise ValueError('invalid source environment schema or pin')
    if type(expected['cases']) is not list or type(actual) is not list:
        raise ValueError('invalid source environment cases')
    cases = [validate(row) for row in expected['cases']]
    observations = [validate(row) for row in actual]
    if [row[0] for row in cases] != LABELS or json.dumps(observations) != json.dumps(cases):
        raise ValueError('fresh source macro observations differ')
    for values in (expected['result'], result):
        if type(values) is not list or any(type(value) is not int for value in values) or values != RESULT:
            raise ValueError('unexpected executed source environment projections')


def main():
    expected = parse((ROOT / 'tests/oracle/source-environment-observations.json').read_text())
    actual = [parse(line) for line in (ROOT / 'tests/oracle/out/source-environment-calls.jsonl').read_text().splitlines()]
    result = parse((ROOT / 'tests/oracle/out/source-environment-result.json').read_text())
    compare(expected, actual, result)
    print('3 fresh pinned source macro projections and 4 executed results match; full portable AST/inference remains pending')


if __name__ == '__main__':
    main()
