#!/usr/bin/env python3
"""Verify eight small exact integer source-position facts, not a general value codec."""
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
for stem, label in (("environment-origin", "source-position"), ("metadata-position", "local-token-position")):
    expected = json.loads((root / f'tests/oracle/{stem}-observations.json').read_text(), object_pairs_hook=unique)
    if set(expected) != {'schema', 'upstream', 'positions'} or type(expected['schema']) is not int or expected['schema'] != 1 or expected['upstream'] != 'c4295f303100bbf5afac449242d30bca1126f1a1':
        raise ValueError('invalid pinned position corpus')
    actual = json.loads((root / f'tests/oracle/out/{stem}-observations.json').read_text())
    for positions in (expected['positions'], actual):
        if not isinstance(positions, list) or len(positions) != 4 or any(not isinstance(pair, list) or len(pair) != 2 or any(type(n) is not int or not 0 < n < 1000 for n in pair) for pair in positions):
            raise ValueError('invalid exact integer source-position facts')
    if actual != expected['positions']:
        raise ValueError(f'changed pinned {label} facts: {actual!r}')
    print(f'4 pinned {label} facts match exactly')
