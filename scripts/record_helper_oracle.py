#!/usr/bin/env python3
"""Closed pinned helper reference; does not certify record/reify or native support."""
import json
from pathlib import Path
import sys
from cljs_inventory import PIN, Scanner

ROOT = Path(__file__).resolve().parents[1]
CASE_IDS = ['vary-meta-args-0', 'vary-meta-args-1', 'vary-meta-args-2', 'vary-meta-args-3', 'vary-meta-args-4', 'vary-meta-args-5', 'vary-meta-effect-once', 'fnil-defaults-1-args-1', 'fnil-defaults-1-args-2', 'fnil-defaults-1-args-3', 'fnil-defaults-1-args-5', 'fnil-defaults-2-args-2', 'fnil-defaults-2-args-3', 'fnil-defaults-2-args-5', 'fnil-defaults-3-args-2', 'fnil-defaults-3-args-3', 'fnil-defaults-3-args-5', 'fnil-false-values', 'fnil-argument-order-and-once', 'fnil-immutable-nil-macro', 'update-in-args-0', 'update-in-args-1', 'update-in-args-2', 'update-in-args-3', 'update-in-args-4', 'update-in-missing-path', 'update-in-empty-path', 'update-in-throw-order', 'group-by-empty', 'group-by-nil-false-keys', 'group-by-callback-order-once', 'group-by-transient-promotion', 'group-by-callback-throw-order']

def unique(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError('duplicate JSON key')
        result[key] = value
    return result

def read(path):
    return json.loads(path.read_text(), object_pairs_hook=unique)

def cases(corpus=None):
    if corpus is None:
        corpus = read(ROOT / 'tests/oracle/record-helper-cases.json')
    if set(corpus) != {'upstream', 'cases'} or corpus['upstream'] != PIN:
        raise ValueError('corpus schema/pin')
    entries = corpus['cases']
    if type(entries) is not list or [case.get('id') for case in entries] != CASE_IDS:
        raise ValueError('closed ordered case identities')
    for case in entries:
        if set(case) != {'id', 'source', 'expected'} or type(case['source']) is not str or not case['source'].strip():
            raise ValueError('case schema/source')
        if type(case['expected']) is not dict or set(case['expected']) != {'tag', 'value'} or case['expected']['tag'] != 'bool' or type(case['expected']['value']) is not bool or case['expected']['value'] is not True:
            raise ValueError('strict Boolean expectation')
        if len(Scanner(case['source']).all()) != 1:
            raise ValueError('case must be one complete source form')
    return entries

def generate():
    entries = '\n'.join('#js {:id ' + json.dumps(case['id']) + ' :value ' + case['source'] + '}' for case in cases())
    path = ROOT / 'tests/oracle/out/generated/suss_oracle/record_helpers.cljs'
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text('(ns suss-oracle.record-helpers)\n(defn -main [] (println (.stringify js/JSON (into-array [' + entries + ']))))\n(set! *main-cli-fn* -main)\n')

def compare(actual=None):
    if actual is None:
        actual = read(ROOT / 'tests/oracle/out/record-helper-observations.json')
    expected = [{'id': case['id'], 'value': case['expected']['value']} for case in cases()]
    if type(actual) is not list or any(type(row) is not dict or set(row) != {'id', 'value'} or type(row['value']) is not bool for row in actual) or actual != expected:
        raise ValueError('complete ordered Boolean observations differ')
    print('PASS 33 pinned record helper observations')

if __name__ == '__main__':
    {'generate': generate, 'compare': compare}[sys.argv[1]]()
