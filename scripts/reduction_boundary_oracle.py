#!/usr/bin/env python3
"""Original development-only Reduced callback boundary oracle."""
import json
import sys
import portable_oracle as oracle

oracle.CORPUS = oracle.ROOT / 'tests/oracle/reduction-boundary-cases.json'
oracle.OBSERVATIONS = oracle.ROOT / 'tests/oracle/out/reduction-boundary-observations.json'

if sys.argv[1:] == ['generate']:
    corpus = oracle.load()
    target = oracle.ROOT / 'tests/oracle/out/generated/suss_oracle/reduction_boundary_cases.cljs'
    target.parent.mkdir(parents=True, exist_ok=True)
    fixture = (oracle.ROOT / 'tests/oracle/fixtures/reduction-boundary-probe.sus').read_text()
    entries = '\n'.join(f'    #js {{:id {json.dumps(case["id"])} :value (encode {case["source"]})}}'
                        for case in corpus['cases'])
    target.write_text('(ns suss-oracle.reduction-boundary-cases)\n' + fixture +
                      '\n(defn observations [encode]\n [\n' + entries + '\n ])\n')
elif sys.argv[1:] == ['compare']:
    oracle.compare()
else:
    raise SystemExit('usage: reduction_boundary_oracle.py generate|compare')
