#!/usr/bin/env python3
"""Generate and compare a bounded source corpus with pinned ClojureScript."""
import json
from pathlib import Path
from oracle_transport import PIN, fields, unique, value

ROOT = Path(__file__).resolve().parents[1]
CORPUS = ROOT / 'tests/oracle/subvector-cases.json'
OBSERVATIONS = ROOT / 'tests/oracle/out/subvector-observations.json'


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


def errors():
    cases = json.loads((ROOT / 'tests/oracle/subvector-errors.json').read_text(), object_pairs_hook=unique)
    if not isinstance(cases, list) or not cases:
        raise ValueError('missing errors')
    ids = set()
    for case in cases:
        fields(case, 'id source message')
        if any(not isinstance(case[k], str) or not case[k] for k in ('id','source','message')) or case['id'] in ids:
            raise ValueError('invalid error case')
        ids.add(case['id'])
    return cases


def error_expected(case):
    raw = case['message'].encode('utf-16-le')
    return {'tag':'string', 'units':[int.from_bytes(raw[i:i+2], 'little') for i in range(0,len(raw),2)]}


def generate():
    corpus = load()
    target = ROOT / 'tests/oracle/out/generated/suss_oracle/subvector_cases.cljc'
    target.parent.mkdir(parents=True, exist_ok=True)
    # Keep this runner's generated fixture extension consistent.
    target.with_suffix('.cljs').unlink(missing_ok=True)
    entries = '\n'.join(f'    #js {{:id {json.dumps(case["id"])} :value (encode {case["source"]})}}'
                        for case in corpus['cases'])
    entries += '\n' + '\n'.join(f'    #js {{:id {json.dumps("error-"+case["id"])} :value (encode (try {case["source"]} (catch :default e (.-message e))))}}' for case in errors())
    entries = entries.replace('suss.core/', 'cljs.core/')
    fixture = (ROOT / 'tests/oracle/fixtures/subvector-probe.sus').read_text()
    target.write_text('(ns suss-oracle.subvector-cases)\n' + fixture + '\n(defn observations [encode]\n  [\n' + entries + '\n  ])\n')


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
        {'id': case['id'], 'value': case['expected']} for case in corpus['cases']] + [{'id':'error-'+case['id'], 'value':error_expected(case)} for case in errors()]}
    if observations != expected:
        raise ValueError('fresh pinned compiler observations differ from reviewed portable corpus')
    print(f"{len(corpus['cases']) + len(errors())} pinned portable source observations match exactly")


if __name__ == '__main__':
    import sys
    if sys.argv[1:] == ['generate']:
        generate()
    elif sys.argv[1:] == ['compare']:
        compare()
    else:
        raise SystemExit('usage: subvector_oracle.py generate|compare')
