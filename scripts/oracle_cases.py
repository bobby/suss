#!/usr/bin/env python3
"""Generate ClojureScript thunks from the same source cases read by Rust."""
import json
from pathlib import Path
from oracle_transport import fields, unique, IDS

ROOT = Path(__file__).resolve().parents[1]


def load(source):
    document = json.loads(source, object_pairs_hook=unique)
    fields(document, 'schema cases')
    if type(document['schema']) is not int or document['schema'] != 1:
        raise ValueError('invalid corpus schema')
    cases = document['cases']
    if not isinstance(cases, list):
        raise ValueError('invalid cases')
    for case in cases:
        fields(case, 'id expr')
        if not isinstance(case['expr'], str) or not case['expr'].strip():
            raise ValueError('missing expression')
    if [c['id'] for c in cases] != list(IDS):
        raise ValueError('missing/duplicate/changed corpus IDs')
    return cases


def main():
    cases = load((ROOT / 'tests/oracle/cases.json').read_text())
    target = ROOT / 'tests/oracle/out/generated/suss_oracle/cases.cljs'
    target.parent.mkdir(parents=True, exist_ok=True)
    thunks = '\n'.join(f'    (observe {json.dumps(c["id"])} (fn [trace] {c["expr"]}))' for c in cases)
    target.write_text('(ns suss-oracle.cases)\n(defn observations [observe]\n  [\n' + thunks + '\n  ])\n')


if __name__ == '__main__':
    main()
