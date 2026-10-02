#!/usr/bin/env python3
"""Compare exact method-role compiler facts, not general runtime values."""
import json
from pathlib import Path
root = Path(__file__).resolve().parents[1]
def unique(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError(f'duplicate key: {key}')
        result[key] = value
    return result
def load(path):
    return json.loads(path.read_text(), object_pairs_hook=unique)
def role(value):
    if not isinstance(value, list) or len(value) != 3 or value[0] not in ('let', 'arg', 'field') or type(value[2]) is not bool:
        raise ValueError('invalid method binding role')
    if value[0] == 'arg':
        if type(value[1]) is not int or value[1] < 0:
            raise ValueError('invalid method argument position')
    elif value[1] is not None:
        raise ValueError('nonargument has argument index')
expected = load(root / 'tests/oracle/method-role-observations.json')
if set(expected) != {'schema', 'upstream', 'calls', 'results'} or type(expected['schema']) is not int or expected['schema'] != 1 or expected['upstream'] != 'c4295f303100bbf5afac449242d30bca1126f1a1':
    raise ValueError('invalid pinned method role corpus')
actual_calls = [json.loads(s, object_pairs_hook=unique) for s in (root / 'tests/oracle/out/method-role-calls.jsonl').read_text().splitlines()]
actual_results = load(root / 'tests/oracle/out/method-role-results.json')
for calls, results in ((expected['calls'], expected['results']), (actual_calls, actual_results)):
    if not isinstance(calls, list) or len(calls) != 6:
        raise ValueError('missing method compiler facts')
    labels = set()
    for call in calls:
        if not isinstance(call, list) or len(call) != 2 or not isinstance(call[0], str) or not call[0] or call[0] in labels or not isinstance(call[1], list) or not call[1]:
            raise ValueError('invalid method fact call')
        labels.add(call[0]); names = []
        for binding in call[1]:
            if not isinstance(binding, list) or len(binding) != 3 or not isinstance(binding[0], str) or not binding[0]:
                raise ValueError('invalid method binding')
            names.append(binding[0]); role(binding[1])
            if binding[2] is not None:
                role(binding[2])
        if names != sorted(set(names)):
            raise ValueError('unordered/duplicate method bindings')
    if not isinstance(results, list) or len(results) != 4 or any(type(v) is not int or v < 0 for v in results):
        raise ValueError('missing/invalid executed method results')
if expected['calls'] != actual_calls or expected['results'] != actual_results:
    raise ValueError('changed pinned method roles or results')
print('6 pinned method-role observations and 4 executed count results match exactly')
