#!/usr/bin/env python3
"""Development-only iteration foundation observations against pinned ClojureScript."""
import json
import sys
import portable_oracle as oracle


def generate():
    corpus = oracle.load()
    target = oracle.ROOT / 'tests/oracle/out/generated/suss_oracle/iteration_cases.cljs'
    target.parent.mkdir(parents=True, exist_ok=True)
    entries = '\n'.join(f'    #js {{:id {json.dumps(case["id"])} :value (encode {case["source"]})}}'
                        for case in corpus['cases'])
    target.write_text('(ns suss-oracle.iteration-cases)\n(defn observations [encode]\n [\n' + entries + '\n ])\n')


if __name__ == '__main__':
    boundary = sys.argv[1:] in (['generate', 'boundary'], ['compare', 'boundary'])
    corpus = 'iteration-length-boundary.json' if boundary else 'iteration-cases.json'
    oracle.CORPUS = oracle.ROOT / 'tests/oracle' / corpus
    observation = 'iteration-length-boundary-observations.json' if boundary else 'iteration-observations.json'
    oracle.OBSERVATIONS = oracle.ROOT / 'tests/oracle/out' / observation
    if sys.argv[1:] in (['generate'], ['generate', 'boundary']):
        generate()
    elif sys.argv[1:] in (['compare'], ['compare', 'boundary']):
        oracle.compare()
    else:
        raise SystemExit('usage: iteration_oracle.py generate|compare [boundary]')
