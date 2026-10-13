#!/usr/bin/env python3
"""Closed tree node candidate reference; no native or general compatibility proof."""
import json
from pathlib import Path
import sys
from cljs_inventory import Scanner
ROOT = Path(__file__).resolve().parents[1]
PIN = 'c4295f303100bbf5afac449242d30bca1126f1a1'
CASE_IDS = ['balance-left-nonred', 'balance-left-red-no-red-child', 'balance-left-outer-red', 'balance-left-inner-red', 'balance-left-outer-priority', 'balance-right-nonred', 'balance-right-red-no-red-child', 'balance-right-outer-red', 'balance-right-inner-red', 'balance-right-outer-priority', 'left-del-red', 'right-del-red', 'left-del-black-sibling', 'right-del-black-sibling', 'left-del-red-black-inner', 'right-del-red-black-inner', 'balance-left-del-invariant', 'balance-right-del-invariant', 'blacknode-key-val-count', 'blacknode-nth-get-invoke', 'blacknode-nth-error', 'blacknode-invoke-error', 'blacknode-meta-with-meta', 'blacknode-stack-conj-empty', 'blacknode-seq-rseq', 'blacknode-assoc-vector', 'blacknode-contains-find', 'blacknode-equiv', 'blacknode-reduce', 'blacknode-indexOf', 'blacknode-replace-color', 'blacknode-hash-cache-zero', 'blacknode-hash-cache-success', 'blacknode-hash-cache-throw', 'rednode-key-val-count', 'rednode-nth-get-invoke', 'rednode-nth-error', 'rednode-invoke-error', 'rednode-meta-with-meta', 'rednode-stack-conj-empty', 'rednode-seq-rseq', 'rednode-assoc-vector', 'rednode-contains-find', 'rednode-equiv', 'rednode-reduce', 'rednode-indexOf', 'rednode-replace-color', 'rednode-hash-cache-zero', 'rednode-hash-cache-success', 'rednode-hash-cache-throw', 'black-blacken-owner-identity', 'black-redden-sharing', 'red-blacken-sharing', 'red-redden-invariant', 'red-add-remove-children', 'black-method-insertion-dispatch', 'red-method-balance-left-outer', 'red-method-balance-left-inner', 'red-method-balance-left-none', 'red-method-balance-right-outer', 'red-method-balance-right-inner', 'red-method-balance-right-none', 'kv-inorder-effects', 'kv-left-reduced-preserves-wrapper', 'kv-root-reduced-preserves-wrapper', 'kv-initial-reduced-leftless', 'kv-original-throw', 'blacknode-kv-method', 'rednode-kv-method', 'blacknode-typed-nth-ignores-public-rebind', 'blacknode-typed-assoc-n-ignores-public-rebind', 'rednode-typed-nth-ignores-public-rebind', 'rednode-typed-assoc-n-ignores-public-rebind', 'comparable-symbol', 'comparable-keyword', 'comparable-subvec', 'comparable-vector', 'comparable-mapentry', 'blacknode-comparable-vector', 'blacknode-comparable-other-node', 'blacknode-comparable-live-delegate', 'blacknode-comparable-rejection-format-order', 'rednode-comparable-vector', 'rednode-comparable-other-node', 'rednode-comparable-live-delegate', 'rednode-comparable-rejection-format-order']

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
    corpus = read(ROOT / 'tests/oracle/tree-node-cases.json')
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
    setup = (ROOT / 'tests/oracle/fixtures/tree-node-probe.sus').read_text()
    header = '(ns suss-oracle.tree-nodes)\n(def BlackNode cljs.core/BlackNode)\n(def RedNode cljs.core/RedNode)\n(def balance-left cljs.core/balance-left)\n(def balance-right cljs.core/balance-right)\n(def balance-left-del cljs.core/balance-left-del)\n(def balance-right-del cljs.core/balance-right-del)\n(def tree-map-kv-reduce cljs.core/tree-map-kv-reduce)\n'
    entries = []
    for case in cases():
        source = case['source']
        entries.append('#js {:id ' + json.dumps(case['id']) + ' :value ' + source + '}')
    return header + setup + '\n(defn -main [] (println (.stringify js/JSON #js [\n' + '\n'.join(entries) + '\n])))\n(set! *main-cli-fn* -main)\n'

def generate():
    (ROOT / 'tests/oracle/src/suss_oracle/tree_nodes.cljs').write_text(driver())

def check():
    if (ROOT / 'tests/oracle/src/suss_oracle/tree_nodes.cljs').read_text() != driver():
        raise ValueError('stale original reference driver')
    print('PASS closed tree node corpus/driver; not executed reference evidence')

def compare(path=None):
    actual = read(path or ROOT / 'tests/oracle/out/tree-node-observations.json')
    expected = [{'id': c['id'], 'value': True} for c in cases()]
    if not isinstance(actual, list) or len(actual) != len(expected):
        raise ValueError('observation count')
    if any(not isinstance(row, dict) or set(row) != {'id', 'value'} or type(row['id']) is not str or type(row['value']) is not bool for row in actual):
        raise ValueError('observation exact schema/types')
    if actual != expected:
        raise ValueError('complete ordered observations differ')
    print('PASS ' + str(len(expected)) + ' pinned tree node candidates')

if __name__ == '__main__':
    {'generate': generate, 'check': check, 'compare': compare}[sys.argv[1]]()
