#!/usr/bin/env python3
"""Development-only mutable nominal fields probes against the pinned compiler."""
import json
import sys
import portable_oracle as oracle

oracle.CORPUS = oracle.ROOT / 'tests/oracle/mutable-field-cases.json'
oracle.OBSERVATIONS = oracle.ROOT / 'tests/oracle/out/mutable-field-observations.json'

if sys.argv[1:] == ['generate']:
    corpus = oracle.load()
    target = oracle.ROOT / 'tests/oracle/out/generated/suss_oracle/mutable_field_cases.cljs'
    target.parent.mkdir(parents=True, exist_ok=True)
    entries = '\n'.join(f'    #js {{:id {json.dumps(case["id"])} :value (encode {case["source"]})}}'
                        for case in corpus['cases'])
    target.write_text('(ns suss-oracle.mutable-field-cases)\n(defn observations [encode]\n [\n' + entries + '\n ])\n')
elif sys.argv[1:] == ['compare']:
    oracle.compare()
else:
    raise SystemExit('usage: mutable_field_oracle.py generate|compare')
