#!/usr/bin/env python3
"""Raw closed observations for full str, StringBuffer and Object storage adapters."""
from contextlib import contextmanager
import json
import sys
import portable_oracle as oracle
from cljs_inventory import Scanner
CASE_IDS = ['str-empty', 'str-scalars', 'str-many', 'str-raw-number-return', 'str-raw-nil-return', 'str-conversion-order', 'str-conversion-throw', 'buffer-zero-nil-first', 'buffer-second-nil-suppresses-rest', 'buffer-append-empty', 'buffer-append-self', 'buffer-clear-set', 'buffer-first-string-later-default-hints', 'buffer-set-default-hint', 'buffer-conversion-mutation-captured-left', 'field-collision-constructor-read-write', 'field-hyphen-callback', 'object-variadic-receiver-rest', 'field-borrowed-callback', 'str-live-recursive-var', 'str-immutable-nil-macro', 'buffer-captured-to-array', 'field-collision-protocol-read-write', 'object-fixed-rest-overlap', 'reserved-field-storage', 'buffer-append-nil', 'buffer-undefined-second-suppresses-rest', 'buffer-set-missing', 'buffer-utf16-length-and-ignored-effects']

CORPUS = oracle.ROOT / 'tests/oracle/string-field-cases.json'
OBSERVATIONS = oracle.ROOT / 'tests/oracle/out/string-field-observations.json'

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
    target = oracle.ROOT / 'tests/oracle/out/generated/suss_oracle/string_field_cases.cljs'
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text('(ns suss-oracle.string-field-cases (:require [goog.string.StringBuffer]))\n(def make-buffer (let [convert to-array] (fn [& xs] (js/Reflect.construct goog.string.StringBuffer (convert xs)))))\n(defn observations [encode]\n [' + entries + '])\n')

def compare():
    corpus()
    with paths():
        oracle.compare()

if __name__ == '__main__':
    {'generate': generate, 'compare': compare}[sys.argv[1]]()
