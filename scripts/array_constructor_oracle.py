#!/usr/bin/env python3
"""Strict pinned reference for Array constructor prerequisites; not native proof."""
import json
from pathlib import Path
import sys
ROOT = Path(__file__).resolve().parents[1]
CASE_IDS = ['canonical-array-type', 'zero-arity', 'single-numeric-length-holes', 'single-false-element', 'single-nil-element', 'multiple-elements', 'single-string-element', 'negative-zero-length', 'invalid-length-negative', 'invalid-length-fractional', 'invalid-length-nan', 'invalid-length-infinite', 'invalid-length-overflow', 'multiple-argument-effect-order', 'single-array-element-identity', 'single-object-element-identity', 'single-undefined-element', 'holes-have-no-own-index', 'explicit-undefined-has-own-index', 'valid-sparse-length-above-resource-cap', 'maximum-valid-sparse-length', 'uint32-max-is-ordinary-property']

def unique(pairs):
    out = {}
    for key, value in pairs:
        if key in out:
            raise ValueError('duplicate JSON key: ' + key)
        out[key] = value
    return out

def read(path):
    return json.loads(path.read_text(), object_pairs_hook=unique)

def cases():
    corpus = read(ROOT / 'tests/oracle/array-constructor-cases.json')
    if set(corpus) != {'upstream', 'cases'} or corpus['upstream'] != 'c4295f303100bbf5afac449242d30bca1126f1a1':
        raise ValueError('corpus schema/pin')
    entries = corpus['cases']
    if not isinstance(entries, list) or len(entries) != 22:
        raise ValueError('case count')
    ids = set()
    for case in entries:
        if set(case) != {'id', 'source', 'expected'} or not isinstance(case['id'], str) or not case['id'] or case['id'] in ids:
            raise ValueError('case identity/schema')
        ids.add(case['id'])
        if not isinstance(case['source'], str) or not case['source'] or case['expected'] != {'tag': 'bool', 'value': True} or type(case['expected']['value']) is not bool:
            raise ValueError('case source/expected')
    if [c['id'] for c in entries] != CASE_IDS:
        raise ValueError('closed ordered case identities')
    return entries

def generate():
    entries = '\n'.join('#js {:id ' + json.dumps(c['id']) + ' :value ' + c['source'] + '}' for c in cases())
    fixture = ''
    target = ROOT / 'tests/oracle/out/generated/suss_oracle/array_constructor.cljs'
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text('(ns suss-oracle.array-constructor)\n' + fixture + '\n(defn -main [] (println (.stringify js/JSON (into-array [' + entries + ']))))\n(set! *main-cli-fn* -main)\n')

def compare():
    actual = read(ROOT / 'tests/oracle/out/array-constructor-observations.json')
    expected = [{'id': c['id'], 'value': c['expected']['value']} for c in cases()]
    if actual != expected or not isinstance(actual, list) or any(type(row.get('value')) is not bool for row in actual):
        raise ValueError('complete ordered Boolean observations differ')
    print('PASS 22 pinned Array constructor observations')

if __name__ == '__main__':
    {'generate': generate, 'compare': compare}[sys.argv[1]]()
