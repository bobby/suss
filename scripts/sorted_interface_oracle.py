#!/usr/bin/env python3
"""Pinned sorted-protocol observations using the shared original nominal fixture."""
import json
import sys
import portable_oracle as oracle

oracle.CORPUS = oracle.ROOT / 'tests/oracle/sorted-interface-cases.json'
oracle.OBSERVATIONS = oracle.ROOT / 'tests/oracle/out/sorted-interface-observations.json'
FIXTURE = oracle.ROOT / 'tests/oracle/fixtures/sorted-interface-probe.sus'


def generate():
    corpus = oracle.load()
    target = oracle.ROOT / 'tests/oracle/out/generated/suss_oracle/sorted_interface_cases.cljs'
    target.parent.mkdir(parents=True, exist_ok=True)
    entries = '\n'.join(f'    #js {{:id {json.dumps(case["id"])} :value (encode {case["source"]})}}'
                        for case in corpus['cases'])
    target.write_text('(ns suss-oracle.sorted-interface-cases)\n(defn observations [encode]\n'
                      + FIXTURE.read_text() + '\n [\n' + entries + '\n ])\n')


if __name__ == '__main__':
    if sys.argv[1:] == ['generate']:
        generate()
    elif sys.argv[1:] == ['compare']:
        oracle.compare()
    else:
        raise SystemExit('usage: sorted_interface_oracle.py generate|compare')
