#!/usr/bin/env python3
"""Strict transport for actual pinned execution of shared syntax quote macros."""
import json
import math
from pathlib import Path
import struct
import sys

ROOT = Path(__file__).resolve().parent.parent
PIN = 'c4295f303100bbf5afac449242d30bca1126f1a1'
MACROS = ['add-one', 'sum-inputs', 'twice', 'data', 'vector-splice', 'inspect-reader']


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
            or type(data['schema']) is not int or data['schema'] != 1 or data['upstream'] != PIN):
        raise ValueError('invalid oracle schema or source pin')
    cases = data['cases']
    if (not isinstance(cases, list) or len(cases) != 1
            or not isinstance(cases[0], list) or len(cases[0]) != 2
            or cases[0][0] != 'execution' or not isinstance(cases[0][1], list)
            or len(cases[0][1]) != 18):
        raise ValueError('invalid execution identity, order or width')
    if any(type(v) not in (int, float) or not math.isfinite(v) for v in cases[0][1]):
        raise ValueError('expected finite numeric observations')
    return data


def compare(actual_raw, expected_raw):
    actual, expected = decode(actual_raw), decode(expected_raw)
    for a, b in zip(actual['cases'][0][1], expected['cases'][0][1]):
        if struct.pack('>d', a) != struct.pack('>d', b):
            raise ValueError('syntax quote execution observation differs')


def main(args):
    if args == ['generate']:
        target = ROOT / 'tests/oracle/out/generated/suss_oracle'
        target.mkdir(parents=True, exist_ok=True)
        (target / 'syntax_quote_macros.clj').write_text(
            '(ns suss-oracle.syntax-quote-macros)\n' + '\n'.join(
                (ROOT / ('tests/oracle/syntax-quote-' + name + '.sus')).read_text() for name in MACROS))
        (target / 'syntax_quote_cases.cljs').write_text(
            '(ns suss-oracle.syntax-quote-cases\n'
            '  (:require-macros [suss-oracle.syntax-quote-macros :refer '
            '[add-one sum-inputs twice quoted-data quoted-vector inspect-reader]]))\n' +
            (ROOT / 'tests/oracle/syntax-quote-fixture.sus').read_text())
    elif args == ['compare']:
        compare((ROOT / 'tests/oracle/out/syntax-quote-observations.json').read_text(),
                (ROOT / 'tests/oracle/syntax-quote-observations.json').read_text())
        print('18 actual pinned syntax quote execution observations match')
    else:
        raise SystemExit('usage: syntax_quote_oracle.py generate|compare')


if __name__ == '__main__':
    main(sys.argv[1:])
