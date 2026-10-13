#!/usr/bin/env python3
"""Closed sorted prerequisite candidate reference; no native or general compatibility proof."""
import json
from pathlib import Path
import sys
from cljs_inventory import Scanner
ROOT = Path(__file__).resolve().parents[1]
PIN = 'c4295f303100bbf5afac449242d30bca1126f1a1'
CASE_IDS = ['symbol-identity', 'symbol-no-namespace', 'symbol-left-namespace-absent', 'symbol-right-namespace-absent', 'symbol-namespace-before-name', 'symbol-namespace-greater', 'symbol-namespace-tie-name', 'symbol-empty-name', 'symbol-utf16', 'symbol-no-public-comparator', 'symbol-identity-skips-fields', 'symbol-repeated-conversion-order', 'symbol-conversion-throw-stops-right', 'keyword-identity', 'keyword-no-namespace', 'keyword-left-namespace-absent', 'keyword-right-namespace-absent', 'keyword-namespace-before-name', 'keyword-namespace-greater', 'keyword-namespace-tie-name', 'keyword-empty-name', 'keyword-utf16', 'keyword-no-public-comparator', 'keyword-identity-skips-fields', 'keyword-repeated-conversion-order', 'keyword-conversion-throw-stops-right', 'ci-empty-no-init', 'ci-single-seed-no-call', 'ci-no-init-ordered', 'ci-empty-init-identity', 'ci-init-ordered', 'ci-reduced-no-init', 'ci-reduced-init-false', 'ci-reduced-init-nil', 'ci-original-reducer-throw', 'ci-live-reduced-predicate', 'ci-count-before-nth-throw', 'ci-live-deref', 'ci-seed-false', 'ci-seed-nil', 'ci-typed-count-ignores-public-rebind', 'ci-typed-nth-ignores-public-rebind', 'symbol-namespace-difference-skips-name', 'symbol-namespace-tie-conversion-before-name', 'keyword-namespace-difference-skips-name', 'keyword-namespace-tie-conversion-before-name', 'ci-immutable-macro-paths']

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
    corpus = read(ROOT / 'tests/oracle/sorted-prerequisite-cases.json')
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
    setup = (ROOT / 'tests/oracle/fixtures/sorted-prerequisite-probe.sus').read_text()
    header = '(ns suss-oracle.sorted-prerequisites)\n(def ci-reduce cljs.core/ci-reduce)\n(def compare-symbols cljs.core/compare-symbols)\n(def compare-keywords cljs.core/compare-keywords)\n'
    entries = []
    for case in cases():
        source = case['source']
        entries.append('#js {:id ' + json.dumps(case['id']) + ' :value ' + source + '}')
    return header + setup + '\n(defn -main [] (println (.stringify js/JSON #js [\n' + '\n'.join(entries) + '\n])))\n(set! *main-cli-fn* -main)\n'

def generate():
    (ROOT / 'tests/oracle/src/suss_oracle/sorted_prerequisites.cljs').write_text(driver())

def check():
    if (ROOT / 'tests/oracle/src/suss_oracle/sorted_prerequisites.cljs').read_text() != driver():
        raise ValueError('stale original reference driver')
    print('PASS closed sorted prerequisite corpus/driver; not executed reference evidence')

def compare(path=None):
    actual = read(path or ROOT / 'tests/oracle/out/sorted-prerequisite-observations.json')
    expected = [{'id': c['id'], 'value': True} for c in cases()]
    if not isinstance(actual, list) or len(actual) != len(expected):
        raise ValueError('observation count')
    if any(not isinstance(row, dict) or set(row) != {'id', 'value'} or type(row['id']) is not str or type(row['value']) is not bool for row in actual):
        raise ValueError('observation exact schema/types')
    if actual != expected:
        raise ValueError('complete ordered observations differ')
    print('PASS ' + str(len(expected)) + ' pinned sorted prerequisite candidates')

if __name__ == '__main__':
    {'generate': generate, 'check': check, 'compare': compare}[sys.argv[1]]()
