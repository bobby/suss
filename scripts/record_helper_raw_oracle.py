#!/usr/bin/env python3
"""Raw helper outputs/traces, independent of the supplementary guest Booleans."""
from contextlib import contextmanager
import json
import sys
import portable_oracle as oracle
from cljs_inventory import Scanner
from record_helper_oracle import CASE_IDS

CORPUS = oracle.ROOT / 'tests/oracle/record-helper-raw-cases.json'
OBSERVATIONS = oracle.ROOT / 'tests/oracle/out/record-helper-raw-observations.json'

@contextmanager
def paths():
    saved = oracle.CORPUS, oracle.OBSERVATIONS
    oracle.CORPUS, oracle.OBSERVATIONS = CORPUS, OBSERVATIONS
    try:
        yield
    finally:
        oracle.CORPUS, oracle.OBSERVATIONS = saved

def corpus():
    with paths():
        result = oracle.load()
    if [case['id'] for case in result['cases']] != CASE_IDS:
        raise ValueError('closed ordered raw helper identities')
    for case in result['cases']:
        if len(Scanner(case['source']).all()) != 1:
            raise ValueError('one complete raw helper form required')
    return result

def generate():
    entries = '\n'.join('#js {:id ' + json.dumps(case['id']) + ' :value (encode ' + case['source'] + ')}' for case in corpus()['cases'])
    target = oracle.ROOT / 'tests/oracle/out/generated/suss_oracle/record_helper_raw_cases.cljs'
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text('(ns suss-oracle.record-helper-raw-cases)\n(defn observations [encode]\n [' + entries + '])\n')

def compare():
    corpus()
    with paths():
        oracle.compare()

if __name__ == '__main__':
    {'generate': generate, 'compare': compare}[sys.argv[1]]()
