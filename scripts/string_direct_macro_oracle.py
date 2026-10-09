#!/usr/bin/env python3
"""Raw pending direct str macro obligations; runtime alias cases are separate."""
from contextlib import contextmanager
import json
import sys
import portable_oracle as oracle
from cljs_inventory import Scanner
CASE_IDS = ['direct-str-raw-number-return', 'direct-str-raw-nil-return']

CORPUS = oracle.ROOT / 'tests/oracle/string-direct-macro-cases.json'
OBSERVATIONS = oracle.ROOT / 'tests/oracle/out/string-direct-macro-observations.json'

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
    target = oracle.ROOT / 'tests/oracle/out/generated/suss_oracle/string_direct_macro_cases.cljs'
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text('(ns suss-oracle.string-direct-macro-cases (:require [goog.string.StringBuffer]))\n(def make-buffer (let [convert to-array] (fn [& xs] (js/Reflect.construct goog.string.StringBuffer (convert xs)))))\n(defn observations [encode]\n [' + entries + '])\n')

def compare():
    corpus()
    with paths():
        oracle.compare()

if __name__ == '__main__':
    {'generate': generate, 'compare': compare}[sys.argv[1]]()
