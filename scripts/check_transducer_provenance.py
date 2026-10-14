#!/usr/bin/env python3
"""Check complete pinned transducer retention; this is not runtime acceptance."""
import json
from pathlib import Path
from core_import import PIN, adapt_form, digest, extract_forms, pinned_file, repository_file

ROOT = Path(__file__).resolve().parents[1]
DIRECTORY = 'docs/compatibility/transducers'
IDS = ['runtime:Many:4262', 'runtime:NONE:4274', 'runtime:Single:4276',
       'runtime:Empty:4295', 'runtime:EMPTY:4306', 'runtime:MultiIterator:4308',
       'runtime:chunkIteratorSeq:4324', 'runtime:TransformerIterator:4335',
       'runtime:transformer-iterator:4374', 'runtime:sequence:4394',
       'runtime:Eduction:11023', 'runtime:eduction:11053',
       'runtime:completing:2695', 'runtime:butlast:9673']


def verify_retention(ledger, originals, files):
    if ledger['upstream'] != PIN or ledger['license'] != 'EPL-1.0':
        raise ValueError('pin/license mismatch')
    entries = ledger['forms']
    if [entry['id'] for entry in entries] != IDS:
        raise ValueError('whole declaration identities/order mismatch')
    names = [entry['retained'].casefold() for entry in entries]
    if len(set(names)) != len(names):
        raise ValueError('retained paths collide on a case-insensitive filesystem')
    for entry in entries:
        original = originals[entry['id']]
        raw = files[entry['retained']]
        if entry['sha256'] != digest(original['form'].encode()):
            raise ValueError('stale original declaration hash')
        if raw != (original['form'] + '\n').encode():
            raise ValueError('whole original retention mismatch')
        if entry['retained-file-sha256'] != digest(raw):
            raise ValueError('stale retained file hash')


def verify():
    ledger = json.loads(repository_file(ROOT, DIRECTORY + '/provenance.json').read_text())
    raw = pinned_file(ROOT, 'src/main/cljs/cljs/core.cljs')
    if ledger['source-file-sha256'] != digest(raw):
        raise ValueError('stale pinned source file hash')
    originals = extract_forms(raw.decode(), 'runtime', ledger['source'])
    files = {entry['retained']: repository_file(ROOT, DIRECTORY + '/' + entry['retained']).read_bytes()
             for entry in ledger['forms']}
    verify_retention(ledger, originals, files)
    recipe = json.loads(repository_file(ROOT, 'docs/compatibility/core-import.json').read_text())
    selections = {entry['id']: entry for entry in recipe['forms']}
    paths = []
    for entry in ledger['forms']:
        selection = selections[entry['id']]
        if selection['source-sha256'] != entry['sha256']:
            raise ValueError('selection does not bind retained original')
        paths.append(selection['patch'].casefold())
        patch = json.loads(repository_file(ROOT, selection['patch']).read_text())
        adapt_form(originals[entry['id']]['form'], entry['name'], patch)
    if len(set(paths)) != len(paths):
        raise ValueError('adaptation paths collide on a case-insensitive filesystem')
    # Retain the complete helper dependencies independently of the iterator graph.
    for name, line in [('constantly', 4464), ('int-array', 3840), ('object-array', 3906)]:
        identity = f'runtime:{name}:{line}'
        original = originals[identity]
        retained = repository_file(ROOT, f'{DIRECTORY}/dependency-{line}-{name}.cljs').read_bytes()
        if retained != (original['form'] + '\n').encode():
            raise ValueError(f'whole dependency retention mismatch: {identity}')
        selection = selections[identity]
        if selection['source-sha256'] != digest(original['form'].encode()):
            raise ValueError(f'stale dependency source hash: {identity}')
        patch = json.loads(repository_file(ROOT, selection['patch']).read_text())
        adapt_form(original['form'], name, patch)
    # The public macro must retain its complete source independently of runtime forms.
    lazy = json.loads(repository_file(ROOT, DIRECTORY + '/lazy-seq-provenance.json').read_text())
    if lazy['upstream'] != PIN or lazy['license'] != 'EPL-1.0':
        raise ValueError('lazy-seq pin/license mismatch')
    macro_source = pinned_file(ROOT, 'src/main/clojure/cljs/core.cljc').decode()
    macro = extract_forms(macro_source, 'macro', lazy['source'])['macro:lazy-seq:2285']['form']
    if lazy['source-form-sha256'] != digest(macro.encode()):
        raise ValueError('stale lazy-seq original form hash')
    retained = repository_file(ROOT, DIRECTORY + '/' + lazy['retained']).read_bytes()
    adapted = repository_file(ROOT, DIRECTORY + '/' + lazy['adapted']).read_bytes()
    if retained != (macro + '\n').encode() or lazy['original-sha256'] != digest(retained):
        raise ValueError('whole lazy-seq retention mismatch')
    if adapted != retained.replace(b'(core/defmacro lazy-seq', b'(defmacro lazy-seq', 1):
        raise ValueError('lazy-seq adaptation changes more than declaration head')
    if lazy['adapted-sha256'] != digest(adapted):
        raise ValueError('stale lazy-seq adaptation hash')
    return 14


if __name__ == '__main__':
    print(f'{verify()} complete transducer declarations, 3 dependencies and public lazy-seq source verified; runtime acceptance remains separate')
