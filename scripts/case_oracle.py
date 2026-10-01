#!/usr/bin/env python3
"""Development-only bootstrap case observations against pinned ClojureScript."""
import json
import sys
import subprocess
from oracle_transport import PIN, fields, unique
import portable_oracle as oracle


def generate():
    corpus = oracle.load()
    target = oracle.ROOT / 'tests/oracle/out/generated/suss_oracle/case_cases.cljs'
    target.parent.mkdir(parents=True, exist_ok=True)
    entries = '\n'.join(f'    #js {{:id {json.dumps(case["id"])} :value (encode {case["source"]})}}'
                        for case in corpus['cases'])
    target.write_text('(ns suss-oracle.case-cases)\n(defn observations [encode]\n [\n' + entries + '\n ])\n')


def boundaries():
    record = json.loads((oracle.ROOT / 'tests/oracle/case-reference-boundaries.json').read_text(), object_pairs_hook=unique)
    fields(record, 'schema upstream cases')
    if type(record['schema']) is not int or record['schema'] != 1 or record['upstream'] != PIN or len(record['cases']) != 1:
        raise ValueError('invalid case boundary schema or pin')
    case = record['cases'][0]
    fields(case, 'id source reference native')
    fields(case['reference'], 'stage kind message')
    fields(case['native'], 'stage message')
    if (case['id'] != 'empty-group' or case['reference']['stage'] != 'node-parse'
            or case['reference']['kind'] != 'SyntaxError' or case['native']['stage'] != 'compile'):
        raise ValueError('unexpected case boundary')
    return case


def generate_boundary():
    case = boundaries()
    target = oracle.ROOT / 'tests/oracle/out/generated/suss_oracle/case_reference_boundary.cljs'
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text('(ns suss-oracle.case-reference-boundary)\n(defn -main [] (println ' + case['source'] + '))\n(set! *main-cli-fn* -main)\n')


def compare_boundary():
    case = boundaries()
    result = subprocess.run(['node', str(oracle.ROOT / 'tests/oracle/out/case-reference-boundary.js')],
                            cwd=oracle.ROOT / 'tests/oracle', text=True, capture_output=True)
    (oracle.ROOT / 'tests/oracle/out/case-reference-boundary.stderr').write_text(result.stderr)
    expected = case['reference']['kind'] + ': ' + case['reference']['message']
    lines = result.stderr.splitlines()
    if (result.returncode != 1 or result.stdout != '' or expected not in lines
            or 'return (17);' not in lines or '^^^^^^' not in lines):
        raise ValueError('case reference boundary changed or unexpectedly passed')
    print('1 exact pinned case reference parse failure verified; not a value match')


if __name__ == '__main__':
    oracle.CORPUS = oracle.ROOT / 'tests/oracle/case-cases.json'
    oracle.OBSERVATIONS = oracle.ROOT / 'tests/oracle/out/case-observations.json'
    action = {'generate': generate, 'compare': oracle.compare,
              'generate-boundary': generate_boundary, 'compare-boundary': compare_boundary}.get(' '.join(sys.argv[1:]))
    if action is None:
        raise SystemExit('usage: case_oracle.py generate|compare|generate-boundary|compare-boundary')
    action()
