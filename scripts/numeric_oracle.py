#!/usr/bin/env python3
"""Generate/check lossless numeric conversion samples against pinned CLJS."""
import json
from pathlib import Path
import re
from oracle_transport import PIN, fields, unique, value

ROOT = Path(__file__).resolve().parents[1]
CORPUS = ROOT / 'tests/oracle/numeric-cases.json'
OBSERVATIONS = ROOT / 'tests/oracle/out/numeric-observations.json'


def validate(corpus):
    fields(corpus, 'schema upstream cases')
    if type(corpus['schema']) is not int or corpus['schema'] != 1 or corpus['upstream'] != PIN:
        raise ValueError('wrong numeric corpus schema or source pin')
    if not isinstance(corpus['cases'], list) or len(corpus['cases']) != 1024:
        raise ValueError('missing numeric samples')
    seen = set()
    for case in corpus['cases']:
        fields(case, 'id bits formatted parsed')
        if case['id'] != case['bits'] or not isinstance(case['bits'], str) or not re.fullmatch('[0-9a-f]{16}', case['bits']) or case['bits'] in seen:
            raise ValueError('invalid/duplicate numeric sample bits')
        seen.add(case['bits'])
        value(case['formatted'])
        value(case['parsed'])
        if case['formatted']['tag'] != 'string' or case['parsed']['tag'] != 'f64':
            raise ValueError('wrong numeric conversion result tag')
    return corpus


def load():
    return validate(json.loads(CORPUS.read_text(), object_pairs_hook=unique))


def generate():
    corpus = load()
    path = ROOT / 'tests/oracle/out/generated/suss_oracle/numeric_cases.cljs'
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text('(ns suss-oracle.numeric-cases)\n(def bits [\n' +
                    '\n'.join('  ' + json.dumps(case['bits']) for case in corpus['cases']) + '\n])\n')


def compare():
    corpus = load()
    observations = validate(json.loads(OBSERVATIONS.read_text(), object_pairs_hook=unique))
    if observations != corpus:
        raise ValueError('fresh pinned numeric conversion observations differ from the reviewed corpus')
    print('1024 pinned numeric conversion samples match exactly')


if __name__ == '__main__':
    import sys
    if sys.argv[1:] == ['generate']:
        generate()
    elif sys.argv[1:] == ['compare']:
        compare()
    else:
        raise SystemExit('usage: numeric_oracle.py generate|compare')
