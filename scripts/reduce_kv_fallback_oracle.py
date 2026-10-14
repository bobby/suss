#!/usr/bin/env python3
"""Generate and strictly compare the bounded public reduce-kv fallback oracle."""
import hashlib
import json
import sys
from pathlib import Path

from oracle_transport import PIN, fields, unique, value

ROOT = Path(__file__).resolve().parents[1]
CORPUS = ROOT / 'tests/oracle/reduce-kv-fallback-cases.json'
OBSERVATIONS = ROOT / 'tests/oracle/reduce-kv-fallback-observations.json'
PROVENANCE = ROOT / 'docs/compatibility/reduce-kv-fallback-provenance.json'
RECEIPT = ROOT / 'tests/oracle/reduce-kv-fallback-receipt.json'
FIXTURE = ROOT / 'tests/oracle/fixtures/reduce-kv-fallback-probe.sus'
OUTPUT = ROOT / 'tests/oracle/out/reduce-kv-fallback-observations.json'
GENERATED = ROOT / 'tests/oracle/out/generated/suss_oracle/reduce_kv_fallback.cljs'
IDS = (
    'public-fallback-selected-and-accumulates-real-map-entries',
    'public-fallback-empty-preserves-init-without-callback',
    'public-fallback-reduced-unwraps-and-stops-in-sequence-order',
    'public-fallback-throw-stops-effects-and-operands-evaluate-left-to-right',
    'public-fallback-normal-accumulation-exposes-ordered-callback-trace',
)


def document(path):
    return json.loads(path.read_text(), object_pairs_hook=unique)


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def load_corpus():
    corpus = document(CORPUS)
    fields(corpus, 'schema upstream cases')
    if type(corpus['schema']) is not int or corpus['schema'] != 1 or corpus['upstream'] != PIN:
        raise ValueError('wrong reduce-kv corpus schema or pin')
    if type(corpus['cases']) is not list or len(corpus['cases']) != len(IDS):
        raise ValueError('reduce-kv corpus must contain exactly five cases')
    for case, identity in zip(corpus['cases'], IDS):
        fields(case, 'id source')
        if case['id'] != identity or type(case['source']) is not str or not case['source'].strip():
            raise ValueError('missing, reordered, duplicate or empty reduce-kv case')
    return corpus


def load_observations():
    observations = document(OBSERVATIONS)
    fields(observations, 'schema upstream cases')
    if type(observations['schema']) is not int or observations['schema'] != 2 or observations['upstream'] != PIN:
        raise ValueError('wrong reduce-kv observation schema or pin')
    if type(observations['cases']) is not list or len(observations['cases']) != len(IDS):
        raise ValueError('reduce-kv observations must contain exactly five cases')
    for case, identity in zip(observations['cases'], IDS):
        fields(case, 'id status value effects')
        if case['id'] != identity or case['status'] != 'value' or type(case['effects']) is not list:
            raise ValueError('invalid/reordered reduce-kv observation')
        value(case['value'])
        for effect in case['effects']:
            value(effect)
        if case['effects']:
            raise ValueError(f'{identity}: callback trace belongs in returned value')
    return observations


def verify_receipt():
    receipt = document(RECEIPT)
    fields(receipt, 'schema upstream oracle_compile_exit oracle_node_exit sha256')
    if (type(receipt['schema']) is not int or receipt['schema'] != 1
            or receipt['upstream'] != PIN
            or receipt['oracle_compile_exit'] != 0
            or receipt['oracle_node_exit'] != 0):
        raise ValueError('wrong reduce-kv reference receipt schema, pin or oracle result')
    required = {
        'reduce-kv-fallback-cases.json': CORPUS,
        'reduce-kv-fallback-observations.json': OBSERVATIONS,
        'reduce-kv-fallback-probe.sus': FIXTURE,
        'reduce-kv-fallback-provenance.json': PROVENANCE,
    }
    if receipt['sha256'] != {name: digest(path) for name, path in required.items()}:
        raise ValueError('reduce-kv receipt hashes do not match checked-in inputs')
    provenance = document(PROVENANCE)
    if provenance['contract']['commit'] != PIN:
        raise ValueError('provenance points at a different ClojureScript source pin')
    for entry in ('contract', 'test_guidance', 'portable_adaptation'):
        record = provenance[entry]
        source = ROOT / record['path']
        if digest(source) != record['sha256']:
            raise ValueError(f'source provenance hash mismatch: {record["path"]}')
    shipped = ROOT / provenance['portable_adaptation']['shipped_wrapper_path']
    if digest(shipped) != provenance['portable_adaptation']['shipped_wrapper_sha256']:
        raise ValueError('shipped public reduce-kv wrapper hash mismatch')
    if digest(FIXTURE) != provenance['fixture']['sha256']:
        raise ValueError('authored fixture hash mismatch')


def generate():
    corpus = load_corpus()
    load_observations()
    verify_receipt()
    fixture = FIXTURE.read_text()
    entries = '\n'.join(
        f'    #js {{:id {json.dumps(case["id"])} :status "value" :value (encode {case["source"]}) :effects #js []}}'
        for case in corpus['cases']
    )
    source = f'''(ns suss-oracle.reduce-kv-fallback)
{fixture}
(defn f64-bits [number]
  (let [view (js/DataView. (js/ArrayBuffer. 8))]
    (.setFloat64 view 0 number false)
    (str (.padStart (.toString (.getUint32 view 0 false) 16) 8 "0")
         (.padStart (.toString (.getUint32 view 4 false) 16) 8 "0"))))
(declare encode)
(defn encoded-items [items] (into-array (map encode items)))
(defn encode [item]
  (cond
    (nil? item) #js {{:tag "nil"}}
    (boolean? item) #js {{:tag "bool" :value item}}
    (number? item) #js {{:tag "f64" :bits (f64-bits item)}}
    (string? item) #js {{:tag "string" :units (into-array (map #(.charCodeAt item %) (range (.-length item))))}}
    (keyword? item) #js {{:tag "keyword" :namespace (encode (namespace item)) :name (encode (name item))}}
    (symbol? item) #js {{:tag "symbol" :namespace (encode (namespace item)) :name (encode (name item))}}
    (vector? item) #js {{:tag "vector" :items (encoded-items item)}}
    (seq? item) #js {{:tag "seq" :items (encoded-items item)}}
    :else (throw (js/Error. (str "unsupported reduce-kv observation: " (type item))))))
(defn -main []
  (let [cases [
{entries}
  ]]
    (println (.stringify js/JSON #js {{:schema 2 :upstream {json.dumps(PIN)} :cases (into-array cases)}}))))
(set! *main-cli-fn* -main)
'''
    GENERATED.parent.mkdir(parents=True, exist_ok=True)
    GENERATED.write_text(source)


def compare():
    load_corpus()
    expected = load_observations()
    verify_receipt()
    actual = document(OUTPUT)
    fields(actual, 'schema upstream cases')
    for case in actual['cases']:
        fields(case, 'id status value effects')
        value(case['value'])
        for effect in case['effects']:
            value(effect)
    if actual != expected:
        raise ValueError('fresh pinned ClojureScript reduce-kv observations differ')
    print('5 pinned reduce-kv fallback reference observations match exactly')


if __name__ == '__main__':
    if sys.argv[1:] == ['generate']:
        generate()
    elif sys.argv[1:] == ['compare']:
        compare()
    else:
        raise SystemExit('usage: reduce_kv_fallback_oracle.py generate|compare')
