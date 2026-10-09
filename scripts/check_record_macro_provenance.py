#!/usr/bin/env python3
"""Verify whole pinned macro retention; this does not establish implementation."""
import json
from pathlib import Path
from core_import import PIN, digest, extract_forms, pinned_file, repository_file
ROOT = Path(__file__).resolve().parents[1]

def verify():
    provenance = json.loads((ROOT / 'docs/compatibility/record-macro-foundations/provenance.json').read_text())
    if provenance['upstream'] != PIN or provenance['license'] != 'EPL-1.0':
        raise ValueError('pin/license mismatch')
    path = 'src/main/clojure/cljs/core.cljc'
    forms = extract_forms(pinned_file(ROOT, path).decode(), 'macro', path)
    names = {'exists?', 'reify', 'prepare-protocol-masks', 'annotate-specs', 'dt->et', 'collect-protocols', 'build-positional-factory', 'validate-fields', 'deftype', 'emit-defrecord', 'build-map-factory', 'defrecord'}
    expected = {f['id'] for f in forms.values() if f['name'] in names}
    entries = provenance['forms']
    if len(entries) != 12 or {f['id'] for f in entries} != expected:
        raise ValueError('closed macro declaration identities')
    for entry in entries:
        form = forms[entry['id']]
        if entry['source'] != 'clojurescript/' + path or entry['line'] != form['line'] or entry['end-line'] != form['end-line']:
            raise ValueError('source range mismatch')
        raw = repository_file(ROOT, entry['retained']).read_bytes()
        if raw != (form['form'] + '\n').encode() or entry['sha256'] != digest(form['form'].encode()) or entry['retained-file-sha256'] != digest(raw):
            raise ValueError('whole source retention/hash mismatch')
    print('Verified 12 complete pinned macro forms; execution remains unverified')

if __name__ == '__main__':
    verify()
