#!/usr/bin/env python3
"""Fresh primary observations for the provenance-tracked bootstrap import.

Uses the existing strict source corpus/transport validator; source calls the
pinned upstream definition, while native tests load the generated Suss artifact.
"""
import json
import sys
import portable_oracle as oracle

oracle.CORPUS = oracle.ROOT / 'tests/oracle/core-import-cases.json'
oracle.OBSERVATIONS = oracle.ROOT / 'tests/oracle/out/core-import-observations.json'

if sys.argv[1:] == ['generate']:
    corpus = oracle.load()
    target = oracle.ROOT / 'tests/oracle/out/generated/suss_oracle/core_import_cases.cljs'
    target.parent.mkdir(parents=True, exist_ok=True)
    entries = '\n'.join(f'    #js {{:id {json.dumps(case["id"])} :value (encode {case["source"]})}}'
                        for case in corpus['cases'])
    target.write_text('(ns suss-oracle.core-import-cases)\n(defn observations [encode]\n [\n' + entries + '\n ])\n')
elif sys.argv[1:] == ['compare']:
    oracle.compare()
else:
    raise SystemExit('usage: core_import_oracle.py generate|compare')
