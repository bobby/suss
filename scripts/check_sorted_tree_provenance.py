#!/usr/bin/env python3
"""Whole sorted source checks, not executable or full acceptance certification."""
import json
import subprocess
from pathlib import Path
from core_import import PIN, Scanner, digest, extract_forms, pinned_file, repository_file, adapt_form
from sorted_tree_adaptations import NAMES, CLASSES, adapt
from check_tree_node_provenance import methods, node_structure
ROOT = Path(__file__).resolve().parents[1]
BASE = '0c98b4f7523aaf369fce6813c63ea8c13b2a2205'
FOLDER = ROOT / 'docs/compatibility/sorted-tree-collections'

def structure(raw):
    top=Scanner(raw).all()[0]
    fields=top.children[2]
    return {'fields':raw[fields.start:fields.end], 'methods':methods(raw),
            'protocol-markers':[c.text for c in top.children[3:] if c.kind=='atom']}

def verify():
    receipt=json.loads((FOLDER/'provenance.json').read_text())
    ledger=json.loads((FOLDER/'dependency-ledger.json').read_text())
    source=pinned_file(ROOT,'src/main/cljs/cljs/core.cljs').decode()
    if receipt['upstream']!=PIN or receipt['base']!=BASE or digest(source.encode())!=receipt['source-file-sha256']:
        raise ValueError('pin/base/source hash')
    if [e['name'] for e in receipt['forms']]!=NAMES:raise ValueError('whole ordered group')
    recipe=json.loads((ROOT/'docs/compatibility/core-import.json').read_text())
    previous=json.loads(subprocess.check_output(['git','show',BASE+':docs/compatibility/core-import.json']))
    if len(previous['forms'])!=347 or recipe['forms'][:347]!=previous['forms'] or [e['id'] for e in recipe['forms'][347:]]!=[e['id'] for e in receipt['forms']]:
        raise ValueError('347 selections/order changed or group mismatch')
    for i in range(347):
        path=f'runtime/core-import/extracted/{i:04}.cljs'
        if (ROOT/path).read_bytes()!=subprocess.check_output(['git','show',BASE+':'+path]):raise ValueError('previous extracted bytes')
    for e in receipt['forms']+receipt['macros']:
        source_path=e['source'].removeprefix('clojurescript/')
        phase='macro' if e in receipt['macros'] else 'runtime'
        original=extract_forms(pinned_file(ROOT,source_path).decode(),phase,source_path)[e['id']]
        raw=original['form']
        if (e['line'],e['end-line'])!=(original['line'],original['end-line']) or digest(raw.encode())!=e['sha256']:
            raise ValueError('complete source/range/hash')
        retained=repository_file(ROOT,e['retained']).read_bytes()
        if retained!=(raw+'\n').encode() or digest(retained)!=e['retained-file-sha256']:raise ValueError('retained original bytes')
        if 'patch' in e:
            patch=json.loads(repository_file(ROOT,e['patch']).read_text())
            adapted=adapt_form(raw,e['name'],patch)
            if adapted!=adapt(raw,e['name']) or digest(adapted.encode())!=e['adapted-form-sha256']:raise ValueError('unlisted whole form adaptation')
            if e['name'] in CLASSES and (structure(adapted)!=structure(raw) or ledger['class-structure'][e['name']]!=structure(raw)):
                raise ValueError('class field/marker/method/arity changed')
    tops=Scanner(source).all()
    for e in receipt['anonymous-parents']:
        matches=[f for f in tops if source.count('\n',0,f.start)+1==e['line']]
        if len(matches)!=1:raise ValueError('complete anonymous parent')
        f=matches[0];raw=source[f.start:f.end];retained=repository_file(ROOT,e['retained']).read_bytes()
        if source.count('\n',0,f.end)+1!=e['end-line'] or digest(raw.encode())!=e['sha256'] or retained!=(raw+'\n').encode() or digest(retained)!=e['retained-file-sha256']:
            raise ValueError('anonymous parent bytes/ranges')
    from sequence_provenance import verify as loader
    loader()
    for name in ['macro.wasm','macro.json','runtime.wasm','runtime.json']:
        p='runtime/bootstrap/'+name
        if (ROOT/p).read_bytes()!=subprocess.check_output(['git','show',BASE+':'+p]):raise ValueError('bootstrap changed during authoring')
    if (ROOT/'runtime/core-import/epl-v10.html').read_bytes()!=pinned_file(ROOT,'epl-v10.html'):raise ValueError('EPL')
    if (FOLDER/'NOTICE').read_text().rstrip('\n')!=source.split('\n\n')[0]:raise ValueError('notice')
    if ledger['native-status']!='uncompiled/unexecuted; bootstrap stale':raise ValueError('native status')
    evidence = FOLDER / 'evidence/receipt.json'
    if evidence.exists():
        record = json.loads(evidence.read_text())
        if record['upstream'] != PIN or record['native'] != 'authored/uncompiled/unexecuted' or record['reference']['cases'] != 142:
            raise ValueError('reference/native status')
        for path, expected in record['sha256'].items():
            if path.startswith('tests/oracle/out/'): continue
            if digest((ROOT / path).read_bytes()) != expected: raise ValueError('sorted evidence receipt hash')
        from sorted_tree_oracle import check as raw_check, compare as raw_compare
        raw_check(); raw_compare(FOLDER / 'evidence/observations.json')
    sharing = FOLDER / 'evidence/sharing-repair/receipt.json'
    if sharing.exists():
        record = json.loads(sharing.read_text())
        if record['upstream'] != PIN or record['native'] != 'authored/uncompiled/unexecuted' or record['reference']['exit'] != 0:
            raise ValueError('sharing reference/native status')
        for path, expected in record['sha256'].items():
            if path.startswith('tests/oracle/out/'): continue
            if digest((ROOT / path).read_bytes()) != expected:
                raise ValueError('sharing receipt hash')
        from sorted_tree_sharing_oracle import check as sharing_check
        sharing_check(FOLDER / 'evidence/sharing-repair/observations.json')
    print('Verified 35 complete forms, every field/protocol/method/arity, 7 anonymous parents, 15 macros, loader, EPL and 347 unchanged selections/extracts; native pending')
if __name__=='__main__':verify()
