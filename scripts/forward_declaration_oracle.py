#!/usr/bin/env python3
"""Development-only forward declaration observations against pinned ClojureScript."""
import json
import sys
import portable_oracle as oracle


def generate():
    corpus = oracle.load()
    target = oracle.ROOT / 'tests/oracle/out/generated/suss_oracle/forward_declaration_cases.cljs'
    target.parent.mkdir(parents=True, exist_ok=True)
    entries = '\n'.join(f'    #js {{:id {json.dumps(case["id"])} :value (encode {case["source"]})}}'
                        for case in corpus['cases'])
    target.write_text('(ns suss-oracle.forward-declaration-cases)\n(defn observations [encode]\n [\n' + entries + '\n ])\n')


if __name__ == '__main__':
    oracle.CORPUS = oracle.ROOT / 'tests/oracle/forward-declaration-cases.json'
    oracle.OBSERVATIONS = oracle.ROOT / 'tests/oracle/out/forward-declaration-observations.json'
    if sys.argv[1:] == ['generate']:
        generate()
    elif sys.argv[1:] == ['compare']:
        oracle.compare()
    else:
        raise SystemExit('usage: forward_declaration_oracle.py generate|compare')
