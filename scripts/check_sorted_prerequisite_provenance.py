#!/usr/bin/env python3
"""Verify retained whole sorted prerequisite source; no native execution claim."""
import json
from pathlib import Path
from core_import import PIN, digest, extract_forms, pinned_file, repository_file, adapt_form
ROOT = Path(__file__).resolve().parents[1]
IDS = ['runtime:ci-reduce:1545', 'runtime:compare-symbols:1115', 'runtime:compare-keywords:3462']

def verify():
    path = 'src/main/cljs/cljs/core.cljs'
    raw = pinned_file(ROOT, path)
    forms = extract_forms(raw.decode(), 'runtime', path)
    receipt = json.loads((ROOT / 'docs/compatibility/sorted-comparator-prerequisites/provenance.json').read_text())
    if receipt['upstream'] != PIN or receipt['license'] != 'EPL-1.0' or receipt['source-file-sha256'] != digest(raw):
        raise ValueError('pin/file/license mismatch')
    if [entry['id'] for entry in receipt['forms']] != IDS:
        raise ValueError('closed ordered prerequisite identities')
    for entry in receipt['forms']:
        form = forms[entry['id']]
        if entry['source'] != 'clojurescript/' + path or entry['line'] != form['line'] or entry['end-line'] != form['end-line']:
            raise ValueError('source range mismatch')
        retained = repository_file(ROOT, entry['retained']).read_bytes()
        if retained != (form['form'] + '\n').encode() or entry['sha256'] != digest(form['form'].encode()) or entry['retained-file-sha256'] != digest(retained):
            raise ValueError('whole source/hash mismatch')
        patch = json.loads(repository_file(ROOT, entry['patch']).read_text())
        adapted = adapt_form(form['form'], form['name'], patch)
        if entry['adapted-form-sha256'] != digest(adapted.encode()):
            raise ValueError('adapted whole form/hash mismatch')
    notice = repository_file(ROOT, 'docs/compatibility/sorted-comparator-prerequisites/NOTICE').read_text().rstrip('\n')
    if notice != raw.decode().split('\n\n')[0]:
        raise ValueError('upstream notice mismatch')
    license_bytes = repository_file(ROOT, 'runtime/core-import/epl-v10.html').read_bytes()
    if license_bytes != pinned_file(ROOT, 'epl-v10.html'):
        raise ValueError('packaged upstream license mismatch')
    print('Verified 3 complete pinned sorted prerequisite forms and EPL license; native execution pending')

if __name__ == '__main__': verify()
