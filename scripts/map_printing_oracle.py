#!/usr/bin/env python3
"""Generate and strictly compare pinned map writer/keyword observations."""
import json
import sys
import portable_oracle as oracle

oracle.CORPUS = oracle.ROOT / 'tests/oracle/map-printing-cases.json'
oracle.OBSERVATIONS = oracle.ROOT / 'tests/oracle/out/map-printing-observations.json'

if sys.argv[1:] == ['generate']:
    corpus = oracle.load()
    target = oracle.ROOT / 'tests/oracle/out/generated/suss_oracle/map_printing_cases.cljs'
    target.parent.mkdir(parents=True, exist_ok=True)
    entries = '\n'.join(f'    #js {{:id {json.dumps(case["id"])} :value (encode {case["source"]})}}'
                        for case in corpus['cases'])
    target.write_text('(ns suss-oracle.map-printing-cases)\n(defn observations [encode]\n [\n' + entries + '\n ])\n')
elif sys.argv[1:] == ['compare']:
    oracle.compare()
else:
    raise SystemExit('usage: map_printing_oracle.py generate|compare')
