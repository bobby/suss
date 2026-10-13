#!/usr/bin/env python3
"""Verify whole pinned macro retention; this does not establish implementation."""
import json
import re
from pathlib import Path
from core_import import PIN, digest, extract_forms, pinned_file, repository_file
ROOT = Path(__file__).resolve().parents[1]

def verify_factory_adaptations(provenance=None, host_source=None):
    """Check compiled helper identities and wiring, never claim execution."""
    if provenance is None:
        provenance = json.loads((ROOT / 'docs/compatibility/record-macro-foundations/factory-adaptations.json').read_text())
    if provenance['schema'] != 1 or provenance['upstream'] != PIN or provenance['license'] != 'EPL-1.0':
        raise ValueError('factory adaptation pin/license/schema mismatch')
    if provenance['source'] != 'crates/suss-compile/src/portable_record_factories.rs':
        raise ValueError('factory adaptation source path mismatch')
    raw = repository_file(ROOT, provenance['source']).read_bytes()
    if provenance['source-file-sha256'] != digest(raw):
        raise ValueError('stale factory source file hash')
    text = raw.decode()
    if 'Copyright (c) Rich Hickey' not in text or 'SPDX-License-Identifier: EPL-1.0' not in text:
        raise ValueError('factory source notice/license missing')
    match = re.search(r'SOURCE: &str = r#"(.*?)"#;', text, re.S)
    if match is None or provenance['compiled-source-sha256'] != digest(match.group(1).encode()):
        raise ValueError('stale compiled factory source hash')
    adapted = extract_forms(match.group(1), 'macro', provenance['source'])
    expected = ['macro:build-positional-factory:1760', 'macro:build-map-factory:1960']
    entries = provenance['forms']
    if [entry['id'] for entry in entries] != expected or len(adapted) != 2:
        raise ValueError('closed factory adaptation identities/order mismatch')
    originals = extract_forms(pinned_file(ROOT, 'src/main/clojure/cljs/core.cljc').decode(), 'macro', 'src/main/clojure/cljs/core.cljc')
    for entry in entries:
        original = originals[entry['id']]
        if entry['original-sha256'] != digest(original['form'].encode()):
            raise ValueError('stale factory original hash')
        retained = repository_file(ROOT, entry['original']).read_bytes()
        if retained != (original['form'] + '\n').encode():
            raise ValueError('factory whole original retention mismatch')
        candidates = [form for form in adapted.values() if form['name'] == original['name']]
        if len(candidates) != 1 or entry['adapted-declaration-sha256'] != digest(candidates[0]['form'].encode()):
            raise ValueError('stale adapted factory declaration hash')
    if host_source is None:
        host_source = (ROOT / 'crates/suss-compile/src/portable_macros.rs').read_text()
    if not re.search(r'macros\.session\.eval\(crate::portable_record_factories::SOURCE\)\?', host_source):
        raise ValueError('compiled factory bootstrap installation missing')
    if not re.search(r'"record-factory-source".*?sha256\(crate::portable_record_factories::SOURCE\.as_bytes\(\)\)', host_source, re.S):
        raise ValueError('compiled factory artifact dependency missing')
    return 2

def verify_label_adaptations(provenance=None, host_source=None):
    if provenance is None:
        provenance = json.loads((ROOT / 'docs/compatibility/record-macro-foundations/label-adaptations.json').read_text())
    if provenance['schema'] != 1 or provenance['upstream'] != PIN or provenance['license'] != 'EPL-1.0':
        raise ValueError('record label adaptation pin/license/schema mismatch')
    if provenance['source'] != 'crates/suss-compile/src/portable_record_label.rs':
        raise ValueError('record label source path mismatch')
    raw = repository_file(ROOT, provenance['source']).read_bytes()
    if provenance['source-file-sha256'] != digest(raw):
        raise ValueError('stale record label source file hash')
    text = raw.decode()
    if 'Copyright (c) Rich Hickey' not in text or 'SPDX-License-Identifier: EPL-1.0' not in text:
        raise ValueError('record label source notice missing')
    source = re.search(r'SOURCE: &str = r###"(.*?)"###;', text, re.S)
    if source is None or provenance['compiled-source-sha256'] != digest(source.group(1).encode()):
        raise ValueError('stale compiled record label source hash')
    expected = ['core-CHAR_MAP-377.cljs', 'analyzer-js-reserved-190.cljc', 'compiler-munge-reserved-108.cljc', 'compiler-munge-114.cljc']
    prefix = 'docs/compatibility/record-analyzer-context/'
    if [entry['path'] for entry in provenance['originals']] != [prefix + name for name in expected]:
        raise ValueError('closed record label input identities/order mismatch')
    for entry in provenance['originals']:
        if digest(repository_file(ROOT, entry['path']).read_bytes()) != entry['sha256']:
            raise ValueError('stale retained record label input hash')
    if host_source is None:
        host_source = (ROOT / 'crates/suss-compile/src/portable_macros.rs').read_text()
    if 'macros.session.eval(crate::portable_record_label::SOURCE)?;' not in host_source:
        raise ValueError('record label bootstrap installation missing')
    if not re.search(r'"record-label-source".*?sha256\(crate::portable_record_label::SOURCE\.as_bytes\(\)\)', host_source, re.S):
        raise ValueError('record label artifact dependency missing')
    return 4

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
    verify_factory_adaptations()
    verify_label_adaptations()
    print('Verified 12 complete pinned macro forms, 2 compiled factory adaptations and record label identities; execution remains unverified')

if __name__ == '__main__':
    verify()
