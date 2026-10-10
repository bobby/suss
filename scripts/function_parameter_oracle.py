#!/usr/bin/env python3
"""Independent raw function-parameter values and effect traces."""
from contextlib import contextmanager
import json
import sys
import portable_oracle as oracle
from cljs_inventory import Scanner
CASE_IDS = ['vector-missing', 'vector-nested', 'vector-rest-empty', 'vector-rest-nested', 'vector-as', 'variadic-rest-pattern', 'map-keys', 'map-qualified-keys', 'map-syms', 'map-strs', 'map-nested', 'map-as', 'map-default-eager', 'map-default-nil-false', 'map-seq-pairs', 'map-single-map', 'map-empty-seq', 'map-trailing-map', 'recur-vector', 'recur-map', 'multi-arity-pattern', 'argument-evaluation-once', 'map-large-order', 'map-shorthand-promotion-order', 'map-dissoc-keeps-hash-order', 'default-throw-and-recovery', 'live-nth-order', 'live-get-arities', 'live-rest-next-before-nesting', 'recur-defaults-repeat', 'pinned-identifier-hash-collision', 'symbol-collision-insertion-with-removal', 'symbol-collision-reversed-with-removal', 'nested-vector-collision-promotion-removal', 'nested-map-pattern-key-hash-order', 'array-node-promoted-shorthand-order', 'default-present-still-throws', 'captured-defaults-and-repeat', 'vector-string-repeated-local']

CORPUS = oracle.ROOT / 'tests/oracle/function-parameter-cases.json'
OBSERVATIONS = oracle.ROOT / 'tests/oracle/out/function-parameter-observations.json'

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
    target = oracle.ROOT / 'tests/oracle/out/generated/suss_oracle/function_parameter_cases.cljs'
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text('(ns suss-oracle.function-parameter-cases)\n(defn observations [encode]\n [' + entries + '])\n')

def compare():
    corpus()
    with paths():
        oracle.compare()

if __name__ == '__main__':
    {'generate': generate, 'compare': compare}[sys.argv[1]]()
