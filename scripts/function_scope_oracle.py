#!/usr/bin/env python3
"""Compare exact named-function facts; not a general runtime value decoder."""
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
expected = load(root / 'tests/oracle/function-scope-observations.json')
if set(expected) != {'schema', 'upstream', 'calls', 'results'} or type(expected['schema']) is not int or expected['schema'] != 1 or expected['upstream'] != 'c4295f303100bbf5afac449242d30bca1126f1a1':
    raise ValueError('invalid pinned function scope corpus')
actual_calls = [json.loads(line, object_pairs_hook=unique) for line in (root / 'tests/oracle/out/function-scope-calls.jsonl').read_text().splitlines()]
actual_results = load(root / 'tests/oracle/out/function-scope-results.json')
for calls, results in ((expected['calls'], expected['results']), (actual_calls, actual_results)):
    if not isinstance(calls, list) or len(calls) != 11:
        raise ValueError('missing function scope observations')
    labels = set()
    for call in calls:
        if not isinstance(call, dict) or set(call) != {'label', 'names', 'locals'} or not isinstance(call['label'], str) or not call['label'] or call['label'] in labels:
            raise ValueError('invalid/duplicate function scope observation')
        labels.add(call['label'])
        for key in ('names', 'locals'):
            if not isinstance(call[key], list) or any(not isinstance(s, str) or not s for s in call[key]):
                raise ValueError('invalid function name facts')
        if call['locals'] != sorted(set(call['locals'])):
            raise ValueError('invalid lexical name set')
    if not isinstance(results, list) or len(results) != 10 or any(not isinstance(names, list) or any(not isinstance(s, str) or not s for s in names) for names in results):
        raise ValueError('missing/invalid executed function name results')
if actual_calls != expected['calls'] or actual_results != expected['results']:
    raise ValueError('changed pinned function scope observations')
print('11 pinned function scope observations and 10 executed name vectors match exactly')
