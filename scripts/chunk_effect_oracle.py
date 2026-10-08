#!/usr/bin/env python3
"""Strict pinned chunk-effect corpus; standalone reference lane, no Cargo."""
import json
from pathlib import Path
import sys
from oracle_transport import PIN, fields, unique, value

ROOT = Path(__file__).resolve().parents[1]
CORPUS = ROOT / 'tests/oracle/chunk-effect-cases.json'
OBSERVATIONS = ROOT / 'tests/oracle/out/chunk-effect-observations.json'
IDS = ('demand-31', 'demand-32', 'demand-33', 'demand-65', 'chunk-retry',
       'reduced-0', 'reduced-31', 'reduced-32', 'concat-boundary')


def decode(raw, observations=False):
    doc = json.loads(raw, object_pairs_hook=unique)
    fields(doc, 'schema upstream cases')
    if type(doc['schema']) is not int or doc['schema'] != 1 or doc['upstream'] != PIN:
        raise ValueError('invalid chunk-effect schema/pin')
    if type(doc['cases']) is not list or len(doc['cases']) != len(IDS):
        raise ValueError('missing chunk-effect cases')
    for case, identity in zip(doc['cases'], IDS):
        fields(case, 'id value' if observations else 'id source expected')
        if case['id'] != identity:
            raise ValueError('changed/reordered/duplicate chunk-effect identity')
        if not observations and (type(case['source']) is not str or not case['source'].strip()):
            raise ValueError('missing chunk-effect source')
        node = case['value'] if observations else case['expected']
        value(node)
        if node['tag'] != 'vector':
            raise ValueError('chunk-effect result must be vector')
    return doc


def compare(actual_raw, expected_raw):
    actual = decode(actual_raw, True)
    expected = decode(expected_raw)
    projected = dict(schema=1, upstream=PIN, cases=[dict(id=c['id'], value=c['expected']) for c in expected['cases']])
    if actual != projected:
        raise ValueError('complete ordered chunk-effect observations differ')


def generate():
    corpus = decode(CORPUS.read_text())
    target = ROOT / 'tests/oracle/out/generated/suss_oracle/chunk_effect_cases.cljs'
    target.parent.mkdir(parents=True, exist_ok=True)
    entries = '\n'.join(f' #js {{:id {json.dumps(c["id"])} :value (encode {c["source"]})}}' for c in corpus['cases'])
    target.write_text('(ns suss-oracle.chunk-effect-cases)\n' +
        (ROOT / 'tests/oracle/fixtures/chunk-effect-probe.sus').read_text() +
        '\n(defn observations [encode]\n [\n' + entries + '\n ])\n')


if __name__ == '__main__':
    if sys.argv[1:] == ['generate']:
        generate()
    elif sys.argv[1:] == ['compare']:
        compare(OBSERVATIONS.read_text(), CORPUS.read_text())
        print('9 complete pinned chunk-effect observations match exactly')
    else:
        raise SystemExit('usage: chunk_effect_oracle.py generate|compare')
