#!/usr/bin/env python3
"""Whole record graph runtime functions, raw reference observations."""
from contextlib import contextmanager
import json
import sys
import portable_oracle as oracle
from cljs_inventory import Scanner
CASE_IDS = ['comp-zero', 'comp-one', 'comp-2-args-0', 'comp-2-args-1', 'comp-2-args-2', 'comp-2-args-3', 'comp-2-args-5', 'comp-3-args-0', 'comp-3-args-1', 'comp-3-args-2', 'comp-3-args-3', 'comp-3-args-5', 'comp-5-args-0', 'comp-5-args-1', 'comp-5-args-2', 'comp-5-args-3', 'comp-5-args-5', 'partial-0-args-0', 'partial-0-args-1', 'partial-0-args-2', 'partial-0-args-3', 'partial-0-args-5', 'partial-1-args-0', 'partial-1-args-1', 'partial-1-args-2', 'partial-1-args-3', 'partial-1-args-5', 'partial-2-args-0', 'partial-2-args-1', 'partial-2-args-2', 'partial-2-args-3', 'partial-2-args-5', 'partial-3-args-0', 'partial-3-args-1', 'partial-3-args-2', 'partial-3-args-3', 'partial-3-args-5', 'partial-5-args-0', 'partial-5-args-1', 'partial-5-args-2', 'partial-5-args-3', 'partial-5-args-5', 'juxt-1-args-0', 'juxt-1-args-1', 'juxt-1-args-2', 'juxt-1-args-3', 'juxt-1-args-5', 'juxt-2-args-0', 'juxt-2-args-1', 'juxt-2-args-2', 'juxt-2-args-3', 'juxt-2-args-5', 'juxt-3-args-0', 'juxt-3-args-1', 'juxt-3-args-2', 'juxt-3-args-3', 'juxt-3-args-5', 'juxt-5-args-0', 'juxt-5-args-1', 'juxt-5-args-2', 'juxt-5-args-3', 'juxt-5-args-5', 'merge-zero', 'merge-all-nil', 'merge-false-nil-values', 'merge-with-zero', 'merge-with-order', 'merge-with-absence', 'update-args-0', 'update-args-1', 'update-args-2', 'update-args-3', 'update-args-4', 'update-missing', 'update-throw-order', 'select-keys-meta-presence', 'select-keys-pinned-sentinel', 'zipmap-empty', 'zipmap-shortest', 'zipmap-duplicate', 'zipmap-promotion', 'gensym-both-arities-counter', 'gensym-counter-reuse', 'gensym-immutable-nil-guard']

CORPUS = oracle.ROOT / 'tests/oracle/record-graph-runtime-cases.json'
OBSERVATIONS = oracle.ROOT / 'tests/oracle/out/record-graph-runtime-observations.json'

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
        raise ValueError('closed ordered runtime graph identities')
    for case in result['cases']:
        if len(Scanner(case['source']).all()) != 1:
            raise ValueError('one complete runtime graph form required')
    return result

def generate():
    entries = '\n'.join('#js {:id ' + json.dumps(case['id']) + ' :value (encode ' + case['source'] + ')}' for case in corpus()['cases'])
    target = oracle.ROOT / 'tests/oracle/out/generated/suss_oracle/record_graph_runtime_cases.cljs'
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text('(ns suss-oracle.record-graph-runtime-cases)\n(defn observations [encode]\n [' + entries + '])\n')

def compare():
    corpus()
    with paths():
        oracle.compare()

if __name__ == '__main__':
    {'generate': generate, 'compare': compare}[sys.argv[1]]()
