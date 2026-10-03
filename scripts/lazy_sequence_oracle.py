#!/usr/bin/env python3
"""Reproduce and compare actual pinned LazySeq execution of shared source."""
import json
import math
from pathlib import Path
import struct
import sys

ROOT = Path(__file__).resolve().parent.parent
PIN = 'c4295f303100bbf5afac449242d30bca1126f1a1'


def decode(raw):
    def unique(pairs):
        result = {}
        for key, value in pairs:
            if key in result:
                raise ValueError('duplicate JSON field')
            result[key] = value
        return result
    data = json.loads(raw, object_pairs_hook=unique)
    if (not isinstance(data, dict) or set(data) != {'schema', 'upstream', 'cases'}
            or type(data['schema']) is not int or data['schema'] != 1
            or data['upstream'] != PIN):
        raise ValueError('invalid oracle schema or source pin')
    cases = data['cases']
    if not isinstance(cases, list) or len(cases) != 4:
        raise ValueError('expected exactly four ordered observations')
    for case, (label, width) in zip(cases, [('realize', 5), ('metadata-retry', 7), ('concat', 15), ('constructors', 23)]):
        if (not isinstance(case, list) or len(case) != 2 or case[0] != label
                or not isinstance(case[1], list) or len(case[1]) != width):
            raise ValueError('invalid case identity, order or width')
        for value in case[1]:
            if type(value) not in (int, float) or not math.isfinite(value):
                raise ValueError('expected finite numeric observation')
    return data


def compare(actual_raw, expected_raw):
    actual, expected = decode(actual_raw), decode(expected_raw)
    for actual_case, expected_case in zip(actual['cases'], expected['cases']):
        for actual_value, expected_value in zip(actual_case[1], expected_case[1]):
            if struct.pack('>d', actual_value) != struct.pack('>d', expected_value):
                raise ValueError(f'{actual_case[0]}: numeric observation differs')


def main(args):
    if args == ['generate']:
        target = ROOT / 'tests/oracle/out/generated/suss_oracle/lazy_sequence_cases.cljs'
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text('(ns suss-oracle.lazy-sequence-cases)\n' +
                          (ROOT / 'tests/oracle/lazy-sequence-fixture.sus').read_text())
    elif args == ['compare']:
        compare((ROOT / 'tests/oracle/out/lazy-sequence-observations.json').read_text(),
                (ROOT / 'tests/oracle/lazy-sequence-observations.json').read_text())
        print('50 actual pinned lazy sequence/constructor observations match')
    else:
        raise SystemExit('usage: lazy_sequence_oracle.py generate|compare')


if __name__ == '__main__':
    main(sys.argv[1:])
