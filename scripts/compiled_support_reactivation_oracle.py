#!/usr/bin/env python3
"""Raw observations for reactivated original compiled support tests."""
from contextlib import contextmanager
import json
import sys
import portable_oracle as oracle
from cljs_inventory import Scanner
CASE_IDS = ['test_comp', 'test_debug_comp_simple', 'test_partial', 'test_fn_map_destructuring_keys', 'test_defn_map_destructuring', 'test_fn_map_destructuring_as', 'test_multi_arity_map_destructuring', 'comp-effect-order-and-capture', 'partial-effect-order-and-capture', 'parameter-eager-default-capture-once', 'multi-arity-zero-and-map']

CORPUS = oracle.ROOT / 'tests/oracle/compiled-support-reactivation-cases.json'
OBSERVATIONS = oracle.ROOT / 'tests/oracle/out/compiled-support-reactivation-observations.json'

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
    target = oracle.ROOT / 'tests/oracle/out/generated/suss_oracle/compiled_support_reactivation_cases.cljs'
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text('(ns suss-oracle.compiled-support-reactivation-cases)\n(defn observations [encode]\n [' + entries + '])\n')

def compare():
    corpus()
    with paths():
        oracle.compare()

if __name__ == '__main__':
    {'generate': generate, 'compare': compare}[sys.argv[1]]()
