#!/usr/bin/env python3
"""Capture and strictly compare the public-record nominal contract in pinned CLJS."""
import hashlib
import json
from pathlib import Path
import sys

from cljs_inventory import Scanner
from oracle_transport import PIN, fields, unique, value

ROOT = Path(__file__).resolve().parents[1]
CORPUS = ROOT / 'tests/oracle/record-nominal-cases.json'
FIXTURE = ROOT / 'tests/oracle/fixtures/public-record-contracts.sus'
RAW = ROOT / 'tests/oracle/out/record-nominal-observations.json'
GENERATED = ROOT / 'tests/oracle/out/generated/app_records.cljs'
FIXTURE_SHA256 = '471fd42576aaabbc7967f4ed25fb0e0159a9d71382b5255f2716aec3832df8a7'
QUERY = ('(let [r (->TestDissocRecord 1 2 nil) '
         'same (->TestDissocRecord 1 2 nil) '
         'other (->OtherRecord 1 2 nil) plain {:a 1 :b 2 :c nil}] '
         '[(= r same) (= same r) (= (hash r) (hash same)) '
         '(= r other) (= other r) (= r plain) (= plain r) '
         '(record? r) (record? other) (record? plain)])')
CASE_ID = 'public-record-nominal-contract'


def read(path):
    return json.loads(path.read_text(), object_pairs_hook=unique)


def corpus():
    document = read(CORPUS)
    fields(document, 'schema upstream fixture fixture_sha256 cases')
    if (type(document['schema']) is not int or document['schema'] != 1
            or document['upstream'] != PIN
            or document['fixture'] != 'tests/oracle/fixtures/public-record-contracts.sus'
            or document['fixture_sha256'] != FIXTURE_SHA256):
        raise ValueError('nominal corpus, upstream pin, or fixture identity changed')
    if hashlib.sha256(FIXTURE.read_bytes()).hexdigest() != FIXTURE_SHA256:
        raise ValueError('public record fixture bytes differ from retained MPL source')
    cases = document['cases']
    if type(cases) is not list or len(cases) != 1:
        raise ValueError('expected the single public nominal contract case')
    case = cases[0]
    fields(case, 'id source expected')
    if case['id'] != CASE_ID or case['source'] != QUERY:
        raise ValueError('nominal case identity or shared expression changed')
    if len(Scanner(case['source']).all()) != 1:
        raise ValueError('the Suss and CLJS query must be one complete expression')
    value(case['expected'])
    expected = {'tag': 'vector', 'items': [
        {'tag': 'bool', 'value': flag}
        for flag in (True, True, True, False, False, False, False, True, True, False)
    ]}
    if case['expected'] != expected:
        raise ValueError('nominal contract must contain exactly the ten Boolean assertions')
    return case


def generate():
    case = corpus()
    fixture = FIXTURE.read_text()
    namespace = '(ns app-records)'
    if fixture.count(namespace) != 1:
        raise ValueError('fixture must retain exactly its original app-records namespace')
    # Retain every fixture form and its MPL provenance; only add the transport
    # require to the existing ns form so the query remains unqualified in app-records.
    fixture = fixture.replace(
        namespace,
        '(ns app-records (:require [suss-oracle.main :as transport]))',
        1,
    )
    generated = (
        fixture + '\n(defn -main []\n'
        '  (let [observed ' + case['source'] + ']\n'
        '    (println (.stringify js/JSON\n'
        '      #js {:schema 1 :upstream "' + PIN + '"\n'
        '           :cases #js [#js {:id "' + CASE_ID + '" :status "value"\n'
        '                         :value (transport/encode observed) :effects #js []}]}))))\n'
        '(set! *main-cli-fn* -main)\n'
    )
    GENERATED.parent.mkdir(parents=True, exist_ok=True)
    GENERATED.write_text(generated)


def compare():
    case = corpus()
    actual = read(RAW)
    fields(actual, 'schema upstream cases')
    if type(actual['schema']) is not int or actual['schema'] != 1 or actual['upstream'] != PIN:
        raise ValueError('invalid raw observation schema or upstream pin')
    rows = actual['cases']
    if type(rows) is not list or len(rows) != 1:
        raise ValueError('fresh pinned CLJS observation is missing or duplicated')
    row = rows[0]
    fields(row, 'id status value effects')
    if row['id'] != CASE_ID or row['status'] != 'value':
        raise ValueError('unexpected nominal oracle identity/status')
    value(row['value'])
    if type(row['effects']) is not list or row['effects']:
        raise ValueError('nominal observation must have no side effects')
    expected = {
        'schema': 1,
        'upstream': PIN,
        'cases': [{'id': CASE_ID, 'status': 'value',
                   'value': case['expected'], 'effects': []}],
    }
    if actual != expected:
        raise ValueError('fresh pinned CLJS nominal observation differs from the case contract')
    print('1 pinned public-record nominal case; all 10 raw Boolean observations match')


if __name__ == '__main__':
    if sys.argv[1:] == ['generate']:
        generate()
    elif sys.argv[1:] == ['compare']:
        compare()
    else:
        raise SystemExit('usage: record_nominal_oracle.py generate|compare')
