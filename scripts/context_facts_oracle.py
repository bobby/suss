#!/usr/bin/env python3
"""Check exact compiler context enums; this is not a general value decoder."""
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
expected = json.loads((root / 'tests/oracle/context-facts-observations.json').read_text(), object_pairs_hook=unique)
if set(expected) != {'schema', 'upstream', 'calls', 'results'} or type(expected['schema']) is not int or expected['schema'] != 1 or expected['upstream'] != 'c4295f303100bbf5afac449242d30bca1126f1a1':
    raise ValueError('invalid pinned context corpus')
actual_calls = [json.loads(line) for line in (root / 'tests/oracle/out/context-facts-calls.jsonl').read_text().splitlines()]
actual_results = json.loads((root / 'tests/oracle/out/context-facts-results.json').read_text())
for calls, results in ((expected['calls'], expected['results']), (actual_calls, actual_results)):
    if not isinstance(calls, list) or len(calls) != 16:
        raise ValueError('missing compiler contexts')
    labels = set()
    for pair in calls:
        if not isinstance(pair, list) or len(pair) != 2 or not isinstance(pair[0], str) or not pair[0] or pair[0] in labels or pair[1] not in ('statement', 'expr', 'return'):
            raise ValueError('invalid/duplicate compiler context')
        labels.add(pair[0])
    if not isinstance(results, list) or len(results) != 9 or any(value not in ('statement', 'expr', 'return') for value in results):
        raise ValueError('missing/invalid context results')
if actual_calls != expected['calls'] or actual_results != expected['results']:
    raise ValueError('changed pinned compiler context facts')
print('16 pinned compiler contexts and 9 context result strings match exactly')
