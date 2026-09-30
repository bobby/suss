#!/usr/bin/env python3
"""Development-only portable comparisons probes against the pinned compiler."""
import json
import sys
from oracle_transport import fields, unique, value
import portable_oracle as oracle


def divergences():
    data = json.loads((oracle.ROOT / "tests/oracle/comparison-capture-divergences.json").read_text(), object_pairs_hook=unique)
    fields(data, "schema upstream cases")
    if type(data["schema"]) is not int or data["schema"] != 1 or data["upstream"] != oracle.PIN or not isinstance(data["cases"], list) or len(data["cases"]) != 4:
        raise ValueError("invalid capture divergence corpus")
    ids = set()
    for case in data["cases"]:
        fields(case, "id source expected-primary expected-native rationale")
        if any(not isinstance(case[key], str) or not case[key].strip() for key in ["id", "source", "rationale"]) or case["id"] in ids:
            raise ValueError("invalid capture divergence case")
        ids.add(case["id"])
        value(case["expected-primary"]); value(case["expected-native"])
        if case["expected-primary"]["tag"] != "string" or case["expected-native"] != {"tag": "bool", "value": True}:
            raise ValueError("invalid capture divergence outcome")
    if ids != {f"{op}-captured-runtime" for op in ["lt", "le", "gt", "ge"]}:
        raise ValueError("missing or unexpected documented capture divergences")
    return data["cases"]


oracle.CORPUS = oracle.ROOT / 'tests/oracle/comparison-cases.json'
oracle.OBSERVATIONS = oracle.ROOT / 'tests/oracle/out/comparison-observations.json'

if sys.argv[1:] == ['generate']:
    corpus = oracle.load()
    target = oracle.ROOT / 'tests/oracle/out/generated/suss_oracle/comparison_cases.cljs'
    target.parent.mkdir(parents=True, exist_ok=True)
    entries = '\n'.join(f'    #js {{:id {json.dumps(case["id"])} :value (encode {case["source"]})}}'
                        for case in corpus['cases'])
    captures = '\n'.join(f'    #js {{:id {json.dumps(case["id"])} :value (try (encode {case["source"]}) (catch :default error (encode (str (.-name error) ": " (.-message error)))))}}' for case in divergences())
    target.write_text('(ns suss-oracle.comparison-cases)\n(defn observations [encode]\n [\n' + entries + '\n ])\n(defn capture-divergences [encode]\n [\n' + captures + '\n ])\n')
elif sys.argv[1:] == ['compare']:
    observations = json.loads(oracle.OBSERVATIONS.read_text(), object_pairs_hook=unique)
    fields(observations, 'schema upstream cases capture-divergences')
    for key in ['cases', 'capture-divergences']:
        if not isinstance(observations[key], list):
            raise ValueError('invalid comparison observations')
        for case in observations[key]:
            fields(case, 'id value'); value(case['value'])
    expected = {'schema': 1, 'upstream': oracle.PIN,
                'cases': [{'id': c['id'], 'value': c['expected']} for c in oracle.load()['cases']],
                'capture-divergences': [{'id': c['id'], 'value': c['expected-primary']} for c in divergences()]}
    if observations != expected:
        raise ValueError('fresh pinned comparison values or documented capture divergences differ')
    print(f"{len(expected['cases'])} exact primary observations; {len(expected['capture-divergences'])} explicit capture divergences reproduced")
else:
    raise SystemExit('usage: comparison_oracle.py generate|compare')
