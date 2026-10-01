#!/usr/bin/env python3
"""Separate pinned wrapper errors and internal-body diagnostics from equal public cases."""
import json
import sys
import portable_oracle as oracle
from oracle_transport import fields, unique, value

CORPUS = oracle.ROOT / 'tests/oracle/bitwise-capture-observations.json'
OBSERVATIONS = oracle.ROOT / 'tests/oracle/out/bitwise-capture-observations.json'


def load():
    corpus = json.loads(CORPUS.read_text(), object_pairs_hook=unique)
    fields(corpus, 'schema upstream cases')
    if type(corpus['schema']) is not int or corpus['schema'] != 1 or corpus['upstream'] != oracle.PIN:
        raise ValueError('invalid bitwise capture schema/pin')
    if not isinstance(corpus['cases'], list):
        raise ValueError('invalid bitwise capture cases')
    ids = set()
    for case in corpus['cases']:
        fields(case, 'id source expected-primary native-source expected-native rationale')
        if any(not isinstance(case[k], str) or not case[k].strip() for k in ['id', 'source', 'native-source', 'rationale']) or case['id'] in ids:
            raise ValueError('invalid bitwise capture case')
        ids.add(case['id'])
        value(case['expected-primary']); value(case['expected-native'])
        if case['expected-native']['tag'] != 'f64' or case['expected-primary']['tag'] != ('string' if case['id'].startswith('public-') else 'f64'):
            raise ValueError('invalid bitwise capture observation type')
    if ids != {f'{mode}-{op}' for mode in ['public', 'body'] for op in ['or', 'and', 'xor', 'and-not']} | {'body-reducer-selected-once'}:
        raise ValueError('missing or unexpected bitwise capture diagnostics')
    return corpus


def generate():
    entries = '\n'.join(f' #js {{:id {json.dumps(c["id"])} :value (try (encode {c["source"]}) (catch :default error (encode (str (.-name error) ": " (.-message error)))))}}' for c in load()['cases'])
    target = oracle.ROOT / 'tests/oracle/out/generated/suss_oracle/bitwise_capture_cases.cljs'
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text('(ns suss-oracle.bitwise-capture-cases)\n(defn observations [encode]\n [\n'+entries+'\n ])\n')


def compare():
    corpus = load()
    expected = {'schema':1, 'upstream':oracle.PIN, 'cases':[{'id':c['id'],'value':c['expected-primary']} for c in corpus['cases']]}
    actual = json.loads(OBSERVATIONS.read_text(), object_pairs_hook=unique)
    fields(actual, 'schema upstream cases')
    if type(actual['schema']) is not int or actual['schema'] != 1 or actual['upstream'] != oracle.PIN or not isinstance(actual['cases'], list):
        raise ValueError('invalid bitwise capture observations')
    for case in actual['cases']:
        fields(case, 'id value'); value(case['value'])
    if actual != expected:
        raise ValueError('pinned bitwise wrapper errors or internal-body diagnostics changed')
    print('4 exact pinned public wrapper errors; 5 separate internal-body diagnostics reproduced')


if __name__ == '__main__':
    if sys.argv[1:] == ['generate']: generate()
    elif sys.argv[1:] == ['compare']: compare()
    else: raise SystemExit('usage: bitwise_capture_oracle.py generate|compare')
