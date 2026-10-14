#!/usr/bin/env python3
"""Generate and compare fresh pinned public IPending value/effect observations."""
import json
from pathlib import Path
import struct
import subprocess
import sys

ROOT = Path(__file__).resolve().parent.parent
PIN = 'c4295f303100bbf5afac449242d30bca1126f1a1'
CASES = [('lazy', 8), ('empty', 5), ('sequence', 9), ('protocol', 3)]


def decode(raw):
    def unique(pairs):
        result = {}
        for key, value in pairs:
            if key in result:
                raise ValueError('duplicate observation field')
            result[key] = value
        return result
    data = json.loads(raw, object_pairs_hook=unique)
    if (set(data) != {'schema', 'upstream', 'cases'}
            or type(data['schema']) is not int or data['schema'] != 1
            or data['upstream'] != PIN or len(data['cases']) != len(CASES)):
        raise ValueError('invalid observation schema or pin')
    for case, (label, width) in zip(data['cases'], CASES):
        if not isinstance(case, list) or len(case) != 2 or case[0] != label or len(case[1]) != width:
            raise ValueError('invalid ordered observation identity or width')
        if any(type(value) not in (int, float) for value in case[1]):
            raise ValueError('expected actual numeric value/effect observations')
    return data


def main(args):
    if args == ['generate']:
        pin = subprocess.check_output(['git', '-C', str(ROOT / 'clojurescript'), 'rev-parse', 'HEAD'], text=True).strip()
        if pin != PIN:
            raise ValueError('unexpected ClojureScript source pin')
        target = ROOT / 'tests/oracle/out/generated/suss_oracle/realized_cases.cljs'
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text('(ns suss-oracle.realized-cases)\n' +
                          (ROOT / 'tests/oracle/realized-fixture.sus').read_text())
    elif args == ['compare']:
        actual = decode((ROOT / 'tests/oracle/out/realized-observations.json').read_text())
        expected = decode((ROOT / 'tests/oracle/realized-observations.json').read_text())
        for a, e in zip(actual['cases'], expected['cases']):
            for av, ev in zip(a[1], e[1]):
                if struct.pack('>d', av) != struct.pack('>d', ev):
                    raise ValueError(f'{a[0]} observation differs')
        print('25 fresh pinned public realization observations match')
    else:
        raise SystemExit('usage: realized_oracle.py generate|compare')


if __name__ == '__main__':
    main(sys.argv[1:])
