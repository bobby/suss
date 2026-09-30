#!/usr/bin/env python3
"""Development-only direct protocol implementation observations against pinned ClojureScript."""
import json
import sys
import portable_oracle as oracle


def generate():
    corpus = oracle.load()
    target = oracle.ROOT / 'tests/oracle/out/generated/suss_oracle/implements_cases.cljs'
    target.parent.mkdir(parents=True, exist_ok=True)
    entries = '\n'.join(f'    #js {{:id {json.dumps(case["id"])} :value (encode {case["source"]})}}'
                        for case in corpus['cases'])
    target.write_text('(ns suss-oracle.implements-cases)\n(defn observations [encode]\n [\n' + entries + '\n ])\n')


if __name__ == '__main__':
    oracle.CORPUS = oracle.ROOT / 'tests/oracle/implements-cases.json'
    oracle.OBSERVATIONS = oracle.ROOT / 'tests/oracle/out/implements-observations.json'
    if sys.argv[1:] == ['generate']:
        generate()
    elif sys.argv[1:] == ['compare']:
        oracle.compare()
    else:
        raise SystemExit('usage: implements_oracle.py generate|compare')
