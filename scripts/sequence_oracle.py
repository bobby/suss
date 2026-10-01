#!/usr/bin/env python3
"""Development-only sequence foundation observations against pinned ClojureScript."""
import json
import sys
import portable_oracle as oracle


def generate():
    corpus = oracle.load()
    target = oracle.ROOT / 'tests/oracle/out/generated/suss_oracle/sequence_cases.cljs'
    target.parent.mkdir(parents=True, exist_ok=True)
    entries = '\n'.join(f'    #js {{:id {json.dumps(case["id"])} :value (encode {case["source"]})}}'
                        for case in corpus['cases'])
    target.write_text('(ns suss-oracle.sequence-cases)\n(defn observations [encode]\n [\n' + entries + '\n ])\n')


if __name__ == '__main__':
    oracle.CORPUS = oracle.ROOT / 'tests/oracle/sequence-cases.json'
    oracle.OBSERVATIONS = oracle.ROOT / 'tests/oracle/out/sequence-observations.json'
    if sys.argv[1:] == ['generate']:
        generate()
    elif sys.argv[1:] == ['compare']:
        oracle.compare()
    else:
        raise SystemExit('usage: sequence_oracle.py generate|compare')
