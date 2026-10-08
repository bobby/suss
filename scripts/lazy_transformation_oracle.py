#!/usr/bin/env python3
"""Full lazy map/filter source corpus, pinned primary lane; no Cargo."""
import json
from pathlib import Path
import sys
import portable_oracle as oracle

oracle.CORPUS = oracle.ROOT / 'tests/oracle/lazy-transformation-cases.json'
oracle.OBSERVATIONS = oracle.ROOT / 'tests/oracle/out/lazy-transformation-observations.json'
IDS = ('map-demand-0', 'map-demand-1', 'map-demand-31', 'map-demand-32', 'map-demand-33', 'map-demand-64', 'map-demand-65', 'map-nonchunk-demand', 'map-nil-false', 'map-two-input-shortest', 'map-three-input-shortest', 'map-four-input-variadic', 'map-five-input-variadic', 'map-transducer-all-reducer-arities', 'filter-transducer-all-reducer-arities', 'filter-nonchunk-nil-false-zero', 'filter-chunk-empty-prefix', 'filter-chunk-all-rejected', 'filter-zero-is-truthy', 'map-throw-false-chunk-throw-retry', 'filter-throw-nil-chunk-throw-retry', 'map-reduced-first', 'filter-reduced-first', 'chunk-buffer-handoff', 'map-ordered-2-inputs', 'map-ordered-3-inputs', 'map-ordered-4-inputs', 'chunk-buffer-sharing-finalization', 'map-transducer-reduced-identity', 'filter-transducer-reduced-identity', 'map-transducer-callback-throw-order', 'filter-transducer-reducer-throw-order', 'map-live-first-helper', 'filter-live-first-helper', 'map-chunk-read-mutation', 'map-live-recursive-helper', 'map-live-chunk-helper', 'filter-chunk-read-mutation', 'filter-live-recursive-helper', 'filter-live-chunk-helper', 'map-transducer-live-variadic-apply')


def corpus():
    result = oracle.load()
    if tuple(case['id'] for case in result['cases']) != IDS:
        raise ValueError('missing/reordered lazy transformation cases')
    return result


if sys.argv[1:] == ['generate']:
    cases = corpus()['cases']
    target = oracle.ROOT / 'tests/oracle/out/generated/suss_oracle/lazy_transformation_cases.cljs'
    target.parent.mkdir(parents=True, exist_ok=True)
    entries = '\n'.join(f' #js {{:id {json.dumps(case["id"])} :value (encode {case["source"]})}}' for case in cases)
    fixture = (oracle.ROOT / 'tests/oracle/fixtures/lazy-transformations-probe.sus').read_text()
    target.write_text('(ns suss-oracle.lazy-transformation-cases)\n' + fixture +
                      '\n(defn observations [encode]\n [\n' + entries + '\n ])\n')
elif sys.argv[1:] == ['compare']:
    corpus()
    oracle.compare()
else:
    raise SystemExit('usage: lazy_transformation_oracle.py generate|compare')
