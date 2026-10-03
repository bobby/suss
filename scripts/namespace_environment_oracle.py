#!/usr/bin/env python3
"""Compare exact fresh namespace entries, nullable map shape, presence and execution."""
import json
from pathlib import Path
from oracle_transport import fields, unique, PIN
ROOT = Path(__file__).resolve().parents[1]
KEYS = ['requires', 'uses', 'renames', 'require-macros', 'use-macros', 'rename-macros']
NAMES = ['graph.app', 'graph.blank', 'graph.nomac']
def parse(text):
    def reject(value): raise ValueError(f'invalid JSON constant: {value}')
    return json.loads(text, object_pairs_hook=unique, parse_constant=reject)
def validate(value):
    fields(value, 'schema upstream namespace maps')
    if type(value['schema']) is not int or value['schema'] != 1 or value['upstream'] != PIN or value['namespace'] not in NAMES:
        raise ValueError('invalid namespace schema, pin or identity')
    if type(value['maps']) is not list or len(value['maps']) != len(KEYS):
        raise ValueError('missing namespace maps')
    for key, item in zip(KEYS, value['maps']):
        if type(item) is not list or len(item) != 3 or item[0] != key or type(item[1]) is not bool:
            raise ValueError('invalid map presence or identity')
        entries = item[2]
        if entries is None: continue
        if not item[1] or type(entries) is not list:
            raise ValueError('invalid namespace map')
        if any(type(entry) is not list or len(entry) != 2 or any(type(s) is not str or not s for s in entry) for entry in entries):
            raise ValueError('invalid namespace entry')
        if entries != sorted(entries) or len({entry[0] for entry in entries}) != len(entries):
            raise ValueError('unordered or duplicate namespace entry')
    return value
expected = parse((ROOT / 'tests/oracle/namespace-environment-observations.json').read_text())
fields(expected, 'schema upstream cases')
if type(expected['schema']) is not int or expected['schema'] != 1 or expected['upstream'] != PIN or type(expected['cases']) is not list:
    raise ValueError('invalid namespace corpus')
expected_cases = [validate(case) for case in expected['cases']]
if [case['namespace'] for case in expected_cases] != NAMES: raise ValueError('invalid expected namespace identities')
actual = [validate(parse(line)) for line in (ROOT / 'tests/oracle/out/namespace-environment-calls.jsonl').read_text().splitlines()]
actual.sort(key=lambda case: NAMES.index(case['namespace']))
if actual != expected_cases: raise ValueError(f'fresh pinned namespace maps differ: {actual}')
result = parse((ROOT / 'tests/oracle/out/namespace-environment-result.json').read_text())
if type(result) is not list or any(type(n) is not int for n in result) or result != [42,42,42]: raise ValueError('unexpected namespace oracle execution')
print('18 pinned namespace map shapes/presence/entries match exactly; three executed results are 42')
