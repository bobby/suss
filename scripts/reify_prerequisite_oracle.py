#!/usr/bin/env python3
"""Strict pinned reference for reify prerequisites; not native proof."""
import json
from pathlib import Path
import sys
ROOT = Path(__file__).resolve().parents[1]
CASE_IDS = ['captured-local', 'with-meta-preserves-capture', 'reader-meta-transfer', 'same-site-nominal-class', 'different-sites-distinct-types', 'protocol-dispatch', 'method-head-recur']

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
    corpus = read(ROOT / 'tests/oracle/reify-prerequisite-cases.json')
    if set(corpus) != {'upstream', 'cases'} or corpus['upstream'] != 'c4295f303100bbf5afac449242d30bca1126f1a1':
        raise ValueError('corpus schema/pin')
    entries = corpus['cases']
    if not isinstance(entries, list) or len(entries) != 7:
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
    target = ROOT / 'tests/oracle/out/generated/suss_oracle/reify_prerequisites.cljs'
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text('(ns suss-oracle.reify-prerequisites)\n' + fixture + '\n(defn -main [] (println (.stringify js/JSON (into-array [' + entries + ']))))\n(set! *main-cli-fn* -main)\n')

def compare():
    actual = read(ROOT / 'tests/oracle/out/reify-prerequisites-observations.json')
    expected = [{'id': c['id'], 'value': c['expected']['value']} for c in cases()]
    if actual != expected or not isinstance(actual, list) or any(type(row.get('value')) is not bool for row in actual):
        raise ValueError('complete ordered Boolean observations differ')
    print('PASS 7 pinned reify prerequisite observations')

if __name__ == '__main__':
    {'generate': generate, 'compare': compare}[sys.argv[1]]()
