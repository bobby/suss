#!/usr/bin/env python3
"""Closed comparator candidate reference; no native or general compatibility proof."""
import json
from pathlib import Path
import sys
from cljs_inventory import Scanner
ROOT = Path(__file__).resolve().parents[1]
PIN = 'c4295f303100bbf5afac449242d30bca1126f1a1'
CASE_IDS = ['identity-object', 'nil-order', 'distinct-nil-undefined', 'number-nan', 'number-infinity', 'numeric-order', 'utf16-order', 'boolean-order', 'arrays-lexical', 'array-holes-nil-undefined', 'array-nested-cycle', 'comparable-return-not-normalized', 'comparable-identity-no-call', 'comparable-original-false', 'numeric-before-comparable', 'array-reconvert-both-relations', 'array-first-relation-only', 'array-conversion-changes-between-tests', 'array-override-valueOf', 'array-type-constructor-nil', 'array-type-constructor-false', 'array-type-constructor-mismatch', 'live-type-mixed', 'live-array-predicate', 'public-primitive-predicate-rebind', 'wrapper-number-unchanged', 'wrapper-truthy-empty', 'wrapper-false-nil-undefined', 'wrapper-reverse-zero-truthy', 'wrapper-reverse-nan-truthy', 'wrapper-reverse-effects', 'wrapper-number-once', 'wrapper-first-throw-no-reverse', 'wrapper-reverse-throw', 'wrapper-live-compare', 'wrapper-equality-changes-compare', 'indexed-length-first', 'indexed-four-arity', 'indexed-stops-first-difference', 'indexed-live-compare', 'format-toString-object-fallback', 'format-noncallable-toString', 'format-order', 'format-throw-stops-second', 'format-live-str', 'str-nil-single-variadic-rebind', 'str-second-lookup-after-first', 'str-nonprimitive-both-throw', 'format-live-str-results-convert-after-calls']

def unique(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError('duplicate JSON key: ' + key)
        result[key] = value
    return result

def read(path):
    return json.loads(path.read_text(), object_pairs_hook=unique)

def cases():
    corpus = read(ROOT / 'tests/oracle/comparator-cases.json')
    if set(corpus) != {'schema', 'upstream', 'cases'} or type(corpus['schema']) is not int or corpus['schema'] != 1 or corpus['upstream'] != PIN:
        raise ValueError('corpus schema/pin')
    entries = corpus['cases']
    if not isinstance(entries, list) or len(entries) != len(CASE_IDS):
        raise ValueError('case count')
    for entry in entries:
        if not isinstance(entry, dict) or set(entry) != {'id', 'source', 'expected'}:
            raise ValueError('case schema')
        if type(entry['id']) is not str or type(entry['source']) is not str or not entry['source'].strip():
            raise ValueError('case identity/source')
        if len(Scanner(entry['source']).all()) != 1:
            raise ValueError('case must be one complete source form')
        expected = entry['expected']
        if not isinstance(expected, dict) or set(expected) != {'tag', 'value'} or expected['tag'] != 'bool' or type(expected['value']) is not bool or expected['value'] is not True:
            raise ValueError('case expected')
    if [c['id'] for c in entries] != CASE_IDS:
        raise ValueError('closed ordered identities')
    return entries

def driver():
    setup = (ROOT / 'tests/oracle/fixtures/comparator-probe.sus').read_text()
    header = '(ns suss-oracle.comparators)\n(def fn->comparator cljs.core/fn->comparator)\n(def compare-indexed cljs.core/compare-indexed)\n(def str_ cljs.core/str_)\n'
    entries = []
    for case in cases():
        source = case['source'].replace('saved str_', 'saved cljs.core/str_').replace('(set! str_', '(set! cljs.core/str_')
        entries.append('#js {:id ' + json.dumps(case['id']) + ' :value ' + source + '}')
    return header + setup + '\n(defn -main [] (println (.stringify js/JSON #js [\n' + '\n'.join(entries) + '\n])))\n(set! *main-cli-fn* -main)\n'

def generate():
    (ROOT / 'tests/oracle/src/suss_oracle/comparators.cljs').write_text(driver())

def check():
    if (ROOT / 'tests/oracle/src/suss_oracle/comparators.cljs').read_text() != driver():
        raise ValueError('stale original reference driver')
    print('PASS closed comparator corpus/driver; not executed reference evidence')

def compare(path=None):
    actual = read(path or ROOT / 'tests/oracle/out/comparator-observations.json')
    expected = [{'id': c['id'], 'value': True} for c in cases()]
    if not isinstance(actual, list) or len(actual) != len(expected):
        raise ValueError('observation count')
    if any(not isinstance(row, dict) or set(row) != {'id', 'value'} or type(row['id']) is not str or type(row['value']) is not bool for row in actual):
        raise ValueError('observation exact schema/types')
    if actual != expected:
        raise ValueError('complete ordered observations differ')
    print('PASS ' + str(len(expected)) + ' pinned comparator candidates')

if __name__ == '__main__':
    {'generate': generate, 'check': check, 'compare': compare}[sys.argv[1]]()
