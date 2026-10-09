#!/usr/bin/env python3
"""Whole tree source/parent/macro/evidence receipts; no native execution claim."""
import json
import re
import subprocess
from pathlib import Path
from core_import import PIN, Scanner, digest, extract_forms, pinned_file, repository_file, adapt_form
ROOT = Path(__file__).resolve().parents[1]
BASE = '5e29a17ee67e6a713c19f080677e2192d2706618'
IDS = ['runtime:balance-left:8404', 'runtime:balance-right:8429',
       'runtime:balance-left-del:8451', 'runtime:balance-right-del:8470',
       'runtime:tree-map-kv-reduce:8489', 'runtime:BlackNode:8502', 'runtime:RedNode:8627']
FOLDER = ROOT / 'docs/compatibility/sorted-tree-nodes'

def methods(raw):
    top = Scanner(raw).all()[0]
    result, section = [], None
    for child in top.children[3:]:
        if child.kind == 'atom': section = child.text
        elif child.kind == 'list':
            result.append({'section': section, 'name': child.children[0].text,
                           'arguments': raw[child.children[1].start:child.children[1].end]})
    return result

def complete_adaptation(original, name):
    """Only the enumerated source expansions are allowed; no branch deletion."""
    adapted = original
    if adapted.startswith('(defn- ' + name + ' '):
        adapted = adapted.replace('(defn- ' + name + ' ', '(def ^:private ' + name + '\n  (fn ', 1) + ')'
    def chain(match):
        value = match.group(1)
        for field in match.group(2).split(): value = '(.' + field + ' ' + value + ')'
        return value
    adapted = re.sub(r'\(\.\. ([a-zA-Z-]+) ((?:-[a-zA-Z]+ ?)+)\)', chain, adapted)
    adapted = adapted.replace('(js/Error.', '(suss.bootstrap/error')
    adapted = adapted.replace('#js [key val]', '(cljs.core/array key val)').replace('#js [val key]', '(cljs.core/array val key)')
    adapted = adapted.replace('(nil? ', '(suss.bootstrap/nil? ').replace('(== ', '(cljs.core/== ')
    adapted = adapted.replace('(caching-hash coll hash-ordered-coll __hash)',
        '(let [cached-hash__ __hash]\n                   (if (suss.bootstrap/nil? cached-hash__)\n                     (let [computed-hash__ (hash-ordered-coll coll)]\n                       (set! __hash computed-hash__)\n                       computed-hash__)\n                     cached-hash__))')
    adapted = adapted.replace('(-nth node ', '(suss.bootstrap/direct-protocol-call IIndexed -nth node ')
    return adapted.replace('(-assoc-n [key val] ', '(suss.bootstrap/direct-protocol-call IVector -assoc-n [key val] ')

def node_structure(raw):
    top = Scanner(raw).all()[0]
    return {'fields': raw[top.children[2].start:top.children[2].end],
            'sections': [child.text for child in top.children[3:] if child.kind == 'atom']}

def verify():
    receipt = json.loads((FOLDER / 'provenance.json').read_text())
    path = 'src/main/cljs/cljs/core.cljs'
    raw = pinned_file(ROOT, path)
    if receipt['upstream'] != PIN or receipt['license'] != 'EPL-1.0' or receipt['source-file-sha256'] != digest(raw):
        raise ValueError('pin/file/license mismatch')
    if [e['id'] for e in receipt['forms']] != IDS: raise ValueError('closed ordered identities')
    original = extract_forms(raw.decode(), 'runtime', path)
    ledger = json.loads((FOLDER / 'dependency-ledger.json').read_text())
    for entry in receipt['forms'] + receipt['macros']:
        source_path = entry['source'].removeprefix('clojurescript/')
        phase = 'macro' if entry in receipt['macros'] else 'runtime'
        form = extract_forms(pinned_file(ROOT, source_path).decode(), phase, source_path)[entry['id']]
        if (entry['line'], entry['end-line']) != (form['line'], form['end-line']): raise ValueError('source range')
        retained = repository_file(ROOT, entry['retained']).read_bytes()
        if retained != (form['form'] + '\n').encode() or entry['sha256'] != digest(form['form'].encode()) or entry['retained-file-sha256'] != digest(retained):
            raise ValueError('complete original bytes/hash')
        if 'patch' in entry:
            patch = json.loads(repository_file(ROOT, entry['patch']).read_text())
            adapted = adapt_form(form['form'], form['name'], patch)
            if entry['adapted-form-sha256'] != digest(adapted.encode()): raise ValueError('adapted hash')
            if adapted != complete_adaptation(form['form'], form['name']): raise ValueError('unlisted expansion or lost source branch')
            if form['name'] in ['BlackNode', 'RedNode']:
                structure = node_structure(form['form'])
                if node_structure(adapted) != structure or ledger['node-structure'][form['name']] != structure:
                    raise ValueError('lost/changed source field, metadata or protocol marker')
                expected = methods(form['form'])
                if methods(adapted) != expected or ledger['node-method-inventory'][form['name']] != expected:
                    raise ValueError('lost/reordered method, protocol or arity')
    from tree_node_registration import expand
    tops = Scanner(raw.decode()).all()
    for entry in receipt['anonymous-parents']:
        matches = [f for f in tops if raw.decode().count('\n', 0, f.start) + 1 == entry['line']]
        if len(matches) != 1: raise ValueError('whole anonymous parent start')
        form = matches[0]; text = raw.decode()[form.start:form.end]
        if raw.decode().count('\n', 0, form.end) + 1 != entry['end-line']: raise ValueError('whole parent end')
        retained = repository_file(ROOT, entry['retained']).read_bytes()
        if retained != (text + '\n').encode() or digest(text.encode()) != entry['sha256'] or digest(retained) != entry['retained-file-sha256']:
            raise ValueError('whole anonymous parent/hash')
        if entry['label'] == 'full-IComparable':
            patch = json.loads(repository_file(ROOT, entry['patch']).read_text())
            if patch['source-sha256'] != entry['sha256'] or patch['replacement'] != expand(text) or digest(patch['replacement'].encode()) != entry['adapted-form-sha256']:
                raise ValueError('whole seven-target adaptation/hash')
    from sequence_provenance import verify as verify_loader
    verify_loader()
    for name in ['macro.wasm', 'macro.json', 'runtime.wasm', 'runtime.json']:
        path = 'runtime/bootstrap/' + name
        if (ROOT / path).read_bytes() != subprocess.check_output(['git', 'show', BASE + ':' + path]):
            raise ValueError('bootstrap artifact changed during authoring')
    if (FOLDER / 'NOTICE').read_text().rstrip('\n') != raw.decode().split('\n\n')[0]: raise ValueError('notice')
    if (ROOT / 'runtime/core-import/epl-v10.html').read_bytes() != pinned_file(ROOT, 'epl-v10.html'): raise ValueError('EPL')
    recipe = json.loads((ROOT / 'docs/compatibility/core-import.json').read_text())
    previous = json.loads(subprocess.check_output(['git', 'show', BASE + ':docs/compatibility/core-import.json']))
    if len(previous['forms']) != 340 or recipe['forms'][:340] != previous['forms'] or [e['id'] for e in recipe['forms'][340:347]] != IDS:
        raise ValueError('old selections changed/order or appended group mismatch')
    for i in range(340):
        path = f'runtime/core-import/extracted/{i:04d}.cljs'
        if (ROOT / path).read_bytes() != subprocess.check_output(['git', 'show', BASE + ':' + path]): raise ValueError('old extract changed')
    # Retained evidence bytes are bound to the exact current authoring inputs.
    for filename, path in [('cases.json', 'tests/oracle/tree-node-cases.json'),
                           ('probe.sus', 'tests/oracle/fixtures/tree-node-probe.sus'),
                           ('tree_nodes.cljs', 'tests/oracle/src/suss_oracle/tree_nodes.cljs')]:
        if (FOLDER / 'evidence' / filename).read_bytes() != (ROOT / path).read_bytes(): raise ValueError('stale reference inputs')
    hashes = json.loads((FOLDER / 'evidence/hashes.json').read_text())
    files = {p.name for p in (FOLDER / 'evidence').iterdir()} - {'hashes.json', 'raw-observations'}
    if files != set(hashes): raise ValueError('evidence receipt file set')
    for name, entry in hashes.items():
        payload = (FOLDER / 'evidence' / name).read_bytes()
        if entry != {'sha256': digest(payload), 'bytes': len(payload)}: raise ValueError('evidence hash/bytes')
    raw_folder = FOLDER / 'evidence/raw-observations'
    if raw_folder.exists():
        raw_receipt = json.loads((raw_folder / 'receipt.json').read_text())
        if raw_receipt['upstream'] != PIN: raise ValueError('raw reference pin')
        for path, expected in raw_receipt['sha256'].items():
            # Generated development JS is not packaged; all retained inputs are mandatory.
            if path.startswith('tests/oracle/out/'): continue
            if digest((ROOT / path).read_bytes()) != expected: raise ValueError('raw reference hash')
        from tree_node_raw_oracle import check as raw_check, compare as raw_compare
        raw_check(); raw_compare(raw_folder / 'observations.json')
    from tree_node_oracle import compare
    compare(FOLDER / 'evidence/observations.json')
    print('Verified 7 whole declarations, every node method/arity, 4 full anonymous parents, complete seven-target registration, 8 macro originals, EPL, 340 unchanged selections/extracts and reference receipts; native pending')

if __name__ == '__main__': verify()
