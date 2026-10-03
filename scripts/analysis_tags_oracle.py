#!/usr/bin/env python3
"""Exact development-only analyzer observations, not a native inference claim."""
import json
from pathlib import Path
from declaration_environment_oracle import parse, validate_data
from oracle_transport import fields, PIN

ROOT = Path(__file__).resolve().parents[1]
KEYS = ['op', 'tag', 'ret-tag', 'inferred-ret-tag', 'children']
LABELS = ['nil', 'boolean', 'number', 'string', 'keyword', 'quoted-symbol',
          'vector', 'map', 'set', 'quoted-list', 'empty-list',
          'arithmetic-string-storage', 'do', 'let', 'loop', 'if-constant-true',
          'if-constant-false', 'if-unknown-union', 'if-unknown-same', 'try-body',
          'var-number', 'var-function', 'dynamic-number', 'invoke-unknown-return',
          'invoke-inferred-number-with-string-storage', 'fn-number', 'fn-unknown',
          'fn-parameter-hint', 'fn-mixed-methods', 'local-number', 'local-hint',
          'local-any']
RESULT = [32, 'a1', 'a1', 42, 10, 11, 1, 's', 1, 1, 8]


def validate(row):
    if type(row) is not list or len(row) != 4 or row[0] not in LABELS:
        raise ValueError('invalid analysis observation identity')
    properties = row[1]
    if type(properties) is not list or len(properties) != len(KEYS):
        raise ValueError('missing analysis fields')
    for key, field in zip(KEYS, properties):
        if (type(field) is not list or len(field) != 3 or field[0] != key
                or type(field[1]) is not bool):
            raise ValueError('invalid analysis field')
        if not field[1] and field[2] is not None:
            raise ValueError('invalid absent analysis field')
        validate_data(field[2])
    operation = properties[0][2]
    if (properties[0][1] is not True or type(operation) is not list
            or len(operation) != 2 or operation[0] != 'keyword'):
        raise ValueError('missing analysis operation')
    for value in row[2:]:
        validate_data(value)
    return row


def compare(expected, actual, result):
    fields(expected, 'schema upstream cases result')
    if type(expected['schema']) is not int or expected['schema'] != 1 or expected['upstream'] != PIN:
        raise ValueError('invalid analysis corpus schema or pin')
    if type(expected['cases']) is not list or type(actual) is not list:
        raise ValueError('invalid analysis corpus cases')
    cases = [validate(row) for row in expected['cases']]
    observations = [validate(row) for row in actual]
    if [row[0] for row in cases] != LABELS or json.dumps(observations) != json.dumps(cases):
        raise ValueError('fresh pinned analysis observations differ')
    # Python equality alone equates Boolean and integer observations.
    for values in (expected['result'], result):
        if (type(values) is not list or len(values) != len(RESULT)
                or any(type(a) is not type(b) or a != b for a, b in zip(values, RESULT))):
            raise ValueError('unexpected analysis oracle execution')


def main():
    expected = parse((ROOT / 'tests/oracle/analysis-tag-observations.json').read_text())
    actual = [parse(line) for line in
              (ROOT / 'tests/oracle/out/analysis-tag-calls.jsonl').read_text().splitlines()]
    result = parse((ROOT / 'tests/oracle/out/analysis-tag-results.json').read_text())
    compare(expected, actual, result)
    print('32 fresh pinned analyzer observations and 11 executed projections match exactly; native execution evidence is checked separately')


if __name__ == '__main__':
    main()
