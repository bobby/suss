#!/usr/bin/env python3
"""Generate and compare a bounded source corpus with pinned ClojureScript."""
import json
from pathlib import Path
from oracle_transport import PIN, fields, unique, value

ROOT = Path(__file__).resolve().parents[1]
CORPUS = ROOT / 'tests/oracle/hash-numeric-boundaries-cases.json'
OBSERVATIONS = ROOT / 'tests/oracle/out/hash-numeric-boundaries-observations.json'


def load():
    corpus = json.loads(CORPUS.read_text(), object_pairs_hook=unique)
    fields(corpus, 'schema upstream cases')
    if type(corpus['schema']) is not int or corpus['schema'] != 1 or corpus['upstream'] != PIN:
        raise ValueError('invalid portable corpus schema or pin')
    if not isinstance(corpus['cases'], list) or not corpus['cases']:
        raise ValueError('missing portable cases')
    identities = set()
    for case in corpus['cases']:
        fields(case, 'id source expected')
        if not isinstance(case['id'], str) or not case['id'] or case['id'] in identities:
            raise ValueError('invalid or duplicate portable case ID')
        identities.add(case['id'])
        if not isinstance(case['source'], str) or not case['source'].strip():
            raise ValueError('missing portable source')
        value(case['expected'])
    return corpus


def generate():
    corpus = load()
    target = ROOT / 'tests/oracle/out/generated/suss_oracle/hash_numeric_boundaries_cases.cljc'
    target.parent.mkdir(parents=True, exist_ok=True)
    # Remove only this runner's old generated .cljs fixture; #? requires .cljc.
    target.with_suffix('.cljs').unlink(missing_ok=True)
    entries = '\n'.join(f'    #js {{:id {json.dumps(case["id"])} :value (encode {case["source"]})}}'
                        for case in corpus['cases'])
    target.write_text('(ns suss-oracle.hash-numeric-boundaries-cases)\n(defn observations [encode]\n  [\n' + entries + '\n  ])\n')


def compare():
    corpus = load()
    observations = json.loads(OBSERVATIONS.read_text(), object_pairs_hook=unique)
    fields(observations, 'schema upstream cases')
    if type(observations['schema']) is not int or observations['schema'] != 1 or observations['upstream'] != PIN:
        raise ValueError('invalid portable observation schema or pin')
    if not isinstance(observations['cases'], list):
        raise ValueError('invalid portable observation cases')
    for case in observations['cases']:
        fields(case, 'id value')
        value(case['value'])
    expected = {'schema': 1, 'upstream': PIN, 'cases': [
        {'id': case['id'], 'value': case['expected']} for case in corpus['cases']]}
    if observations != expected:
        raise ValueError('fresh pinned compiler observations differ from reviewed portable corpus')
    print(f"{len(corpus['cases'])} pinned portable source observations match exactly")


if __name__ == '__main__':
    import sys
    if sys.argv[1:] == ['generate']:
        generate()
    elif sys.argv[1:] == ['compare']:
        compare()
    else:
        raise SystemExit('usage: hash_numeric_boundaries_oracle.py generate|compare')
