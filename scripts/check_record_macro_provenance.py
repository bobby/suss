#!/usr/bin/env python3
"""Verify whole pinned macro retention; this does not establish implementation."""
import json
import re
from pathlib import Path
from core_import import PIN, digest, extract_forms, pinned_file, repository_file
from cljs_inventory import Scanner
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
    if [entry['id'] for entry in entries] != expected:
        raise ValueError('closed factory adaptation identities/order mismatch')
    helpers = provenance.get('helper-declarations', [])
    helper_names = ['factory-join3', 'template-sequence', 'template-constant-0', 'template-constant-1', 'template-constant-2']
    if [entry['name'] for entry in helpers] != helper_names:
        raise ValueError('closed factory helper identities/order mismatch')
    actual_helpers = [form for form in adapted.values()
                      if form['name'] not in ['build-positional-factory', 'build-map-factory']]
    if len(adapted) != 7 or [form['name'] for form in actual_helpers] != helper_names:
        raise ValueError('closed factory declaration set mismatch')
    for entry, form in zip(helpers, actual_helpers):
        if entry['sha256'] != digest(form['form'].encode()):
            raise ValueError('stale factory helper declaration hash')
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
    host_source = re.sub(r'\s+', '', host_source)
    if not re.search(r'macros\.session\.eval\(crate::portable_record_factories::SOURCE\)\?', host_source):
        raise ValueError('compiled factory bootstrap installation missing')
    if not re.search(r'"record-factory-source".*?sha256\(crate::portable_record_factories::SOURCE\.as_bytes\(\)\)', host_source, re.S):
        raise ValueError('compiled factory artifact dependency missing')
    return 2

def verify_label_adaptations(provenance=None, host_source=None, source_bytes=None):
    if provenance is None:
        provenance = json.loads((ROOT / 'docs/compatibility/record-macro-foundations/label-adaptations.json').read_text())
    if provenance['schema'] != 1 or provenance['upstream'] != PIN or provenance['license'] != 'EPL-1.0':
        raise ValueError('record label adaptation pin/license/schema mismatch')
    if provenance['source'] != 'crates/suss-compile/src/portable_record_label.rs':
        raise ValueError('record label source path mismatch')
    raw = repository_file(ROOT, provenance['source']).read_bytes() if source_bytes is None else source_bytes
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
    declarations = provenance.get('declarations', [])
    declaration_names = ['ns:suss.internal.record-label', 'label-join3', 'label-join4',
                         'char-map', 'reserved', 'reserved-word?', 'munge-unit',
                         'replace-dots', 'replace-division', 'mark-reserved', 'type-label']
    if not isinstance(declarations, list) or [entry.get('name') for entry in declarations] != declaration_names:
        raise ValueError('closed record label declaration identities/order mismatch')
    source_text = source.group(1)
    top_forms = Scanner(source_text).all()
    if len(top_forms) != len(declaration_names):
        raise ValueError('record label top-level form count mismatch')
    namespace_form = source_text[top_forms[0].start:top_forms[0].end] if top_forms else ''
    adapted = list(extract_forms(source_text, 'macro', provenance['source']).values())
    if [form['name'] for form in adapted] != declaration_names[1:]:
        raise ValueError('record label source declaration set/order mismatch')
    scanned_forms = [source_text[form.start:form.end] for form in top_forms]
    declaration_forms = [namespace_form] + [form['form'] for form in adapted]
    if scanned_forms != declaration_forms:
        raise ValueError('record label top-level forms must exactly match the closed declaration list')
    if len(set(entry['name'] for entry in declarations)) != len(declarations):
        raise ValueError('duplicate record label declaration identity')
    for entry, form in zip(declarations, declaration_forms):
        if entry.get('sha256') != digest(form.encode()):
            raise ValueError('stale record label declaration hash')
    if host_source is None:
        host_source = (ROOT / 'crates/suss-compile/src/portable_macros.rs').read_text()
    host_source = re.sub(r'\s+', '', host_source)
    if 'macros.session.eval(crate::portable_record_label::SOURCE)?;' not in host_source:
        raise ValueError('record label bootstrap installation missing')
    if not re.search(r'"record-label-source".*?sha256\(crate::portable_record_label::SOURCE\.as_bytes\(\)\)', host_source, re.S):
        raise ValueError('record label artifact dependency missing')
    return 4


def verify_named_property_abi_adaptation(provenance=None, source_bytes=None):
    """Pin the complete ASCII guard and reserved dispatch, not runtime execution."""
    path = 'docs/compatibility/record-macro-foundations/abi-adaptations.json'
    if provenance is None:
        provenance = json.loads((ROOT / path).read_text())
    expected_path = 'crates/suss-compile/src/runtime_abi/named_properties.rs'
    if (provenance.get('schema'), provenance.get('source'), provenance.get('license')) != (
            1, expected_path, 'MIT OR Apache-2.0'):
        raise ValueError('named-property ABI adaptation identity/license mismatch')
    if provenance.get('upstream') != PIN:
        raise ValueError('named-property ABI upstream mismatch')
    expected_whitelist = 'abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_$'
    if provenance.get('allowed-ascii') != expected_whitelist:
        raise ValueError('named-property ABI ASCII whitelist mismatch')
    if provenance.get('algorithm') != 'ascii-masks-lowercase-fastpath-reserved-dispatch':
        raise ValueError('named-property ABI algorithm mismatch')
    anchor = '    // Named instance access must not mistake raw source names for munged fields.\n'
    boundary = '    // Stride one is a field schema, stride two a mixed native/name table.\n'
    if provenance.get('guard-location') != anchor.strip()[3:]:
        raise ValueError('named-property ABI guard location mismatch')
    # Independent of the manifest's refreshable whole-file hash: include setup,
    # all instructions, whitelist construction, reserved names and registration.
    # Keep exact bytes (including comments identifying the pinned upstream).
    expected_guard_hash = 'ec886b0035457d311d74ccd31190a2c86e37f756833920e6a0e4090fa08077b5'
    if provenance.get('guard-sha256') != expected_guard_hash:
        raise ValueError('named-property ABI guard identity mismatch')
    raw = repository_file(ROOT, expected_path).read_bytes() if source_bytes is None else source_bytes
    if provenance.get('source-file-sha256') != digest(raw):
        raise ValueError('stale named-property ABI source hash')
    first, last = anchor.encode(), boundary.encode()
    if raw.count(first) != 1 or raw.count(last) != 1:
        raise ValueError('named-property ABI guard boundaries must occur exactly once')
    start, end = raw.index(first), raw.index(last)
    if end <= start or digest(raw[start:end]) != expected_guard_hash:
        raise ValueError('named-property ABI complete guard differs from its recorded algorithm')
    return 1


def verify_public_adaptations(provenance=None):
    """Bind integrated helper bytes to retained licensed originals."""
    if provenance is None:
        provenance = json.loads((ROOT / 'docs/compatibility/record-macro-foundations/public-adaptations.json').read_text())
    if (provenance['schema'], provenance['upstream'], provenance['license']) != (1, PIN, 'EPL-1.0'):
        raise ValueError('public adaptation pin/license/schema mismatch')
    expected = ['crates/suss-compile/src/portable_record_' + name + '.rs'
                for name in ['protocol_facts', 'emitter', 'public']]
    if [entry['path'] for entry in provenance['sources']] != expected:
        raise ValueError('closed public helper source identities/order mismatch')
    for entry in provenance['sources']:
        raw = repository_file(ROOT, entry['path']).read_bytes()
        if digest(raw) != entry['sha256']:
            raise ValueError('stale public helper source hash')
        text = raw.decode()
        if 'Copyright (c) Rich Hickey' not in text or 'SPDX-License-Identifier: EPL-1.0' not in text:
            raise ValueError('public helper notice/license missing')
        constants = {match.group(1): digest(match.group(3).encode())
                     for match in re.finditer(r'pub\(crate\) const (\w+): &str = r(#+)"(.*?)"\2;', text, re.S)}
        names = {'SOURCE', 'MACRO_SOURCE'} if entry['path'].endswith('_public.rs') else {'SOURCE'}
        if set(constants) != names or constants != entry['compiled-sources']:
            raise ValueError('stale or missing public compiled source identity')
    expected_inputs = ['docs/compatibility/record-macro-foundations/' + name + '.cljc'
                       for name in ['collect-protocols', 'prepare-protocol-masks', 'annotate-specs',
                                    'dt-to-et', 'emit-defrecord', 'validate-fields', 'defrecord']]
    if [entry['path'] for entry in provenance['retained-inputs']] != expected_inputs:
        raise ValueError('closed public retained input identities/order mismatch')
    for entry in provenance['retained-inputs']:
        if digest(repository_file(ROOT, entry['path']).read_bytes()) != entry['sha256']:
            raise ValueError('stale public retained input hash')
    return 3

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
    verify_public_adaptations()
    verify_named_property_abi_adaptation()
    print('Verified 12 complete pinned macro forms, 2 compiled factory adaptations and label/public helper identities; provenance alone does not certify execution')

if __name__ == '__main__':
    verify()
