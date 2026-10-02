#!/usr/bin/env python3
"""Development-only metadata and &form probes against the pinned compiler."""
import json
import sys
import portable_oracle as oracle

oracle.CORPUS = oracle.ROOT / 'tests/oracle/macro-metadata-cases.json'
oracle.OBSERVATIONS = oracle.ROOT / 'tests/oracle/out/macro-metadata-observations.json'

if sys.argv[1:] == ['generate']:
    corpus = oracle.load()
    target = oracle.ROOT / 'tests/oracle/out/generated/suss_oracle/macro_metadata_cases.cljs'
    target.parent.mkdir(parents=True, exist_ok=True)
    entries = '\n'.join(f'    #js {{:id {json.dumps(case["id"])} :value (encode {case["source"]})}}'
                        for case in corpus['cases'])
    target.write_text('(ns suss-oracle.macro-metadata-cases (:require-macros [suss-oracle.macro-metadata-defs :refer [form-tag form-line]]))\n(defn observations [encode]\n [\n' + entries + '\n ])\n')
elif sys.argv[1:] == ['compare']:
    oracle.compare()
else:
    raise SystemExit('usage: macro_metadata_oracle.py generate|compare')
