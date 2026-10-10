#!/usr/bin/env python3
"""Check complete record compiler graph runtime functions against the pin."""
import hashlib
import json
from pathlib import Path
from core_import import ROOT, extract_forms, pinned_file, PIN
from cljs_inventory import Scanner

IDS = ('runtime:comp:4468', 'runtime:partial:4497', 'runtime:juxt:10251',
       'runtime:merge:9265', 'runtime:merge-with:9273', 'runtime:update:5557',
       'runtime:select-keys:9289', 'runtime:zipmap:9690',
       'runtime:gensym_counter:10903', 'runtime:gensym:10907')

def adapt(raw, name):
    if name == 'gensym_counter':
        return raw
    tree = Scanner(raw).all()[0]
    nodes = tree.children
    if nodes[0].text != 'defn' or nodes[1].text != name or nodes[2].kind != 'string':
        raise ValueError('whole declaration shape')
    body = raw[nodes[3].start:tree.end - 1]
    if name == 'merge':
        shorthand = '#(conj (or %1 {}) %2)'
        if body.count(shorthand) != 1: raise ValueError('exact merge shorthand')
        body = body.replace(shorthand, '(fn* [%1 %2] (conj (or %1 {}) %2))')
    if name == 'juxt':
        for expression in ('(%2)', '(%2 x)', '(%2 x y)', '(%2 x y z)', '(apply %2 x y z args)'):
            shorthand = '#(conj %1 ' + expression + ')'
            if body.count(shorthand) != 1: raise ValueError('exact juxt shorthand')
            body = body.replace(shorthand, '(fn* [%1 %2] (conj %1 ' + expression + '))')
    if name == 'select-keys':
        if body.count('::not-found') != 2: raise ValueError('exact sentinel occurrences')
        body = body.replace('::not-found', ':cljs.core/not-found')
    if name == 'gensym':
        if body.count('(nil? gensym_counter)') != 1: raise ValueError('exact nil macro guard')
        body = body.replace('(nil? gensym_counter)', '(suss.bootstrap/nil? gensym_counter)')
    return '(def ' + name + '\n  ' + raw[nodes[2].start:nodes[2].end] + '\n  (fn ' + body + '))'

def verify(recipe=None, patches=None):
    if recipe is None: recipe = json.loads((ROOT / 'docs/compatibility/core-import.json').read_text())
    if recipe['upstream-commit'] != PIN or tuple(entry['id'] for entry in recipe['forms'][340:350]) != IDS:
        raise ValueError('complete ordered runtime graph group')
    if any(sum(entry['id'] == identity for entry in recipe['forms']) != 1 for identity in IDS):
        raise ValueError('duplicate runtime graph selection')
    source = extract_forms(pinned_file(ROOT, 'src/main/cljs/cljs/core.cljs').decode(),
                           'runtime', 'clojurescript/src/main/cljs/cljs/core.cljs')
    for entry in recipe['forms'][340:350]:
        form = source[entry['id']]
        digest = hashlib.sha256(form['form'].encode()).hexdigest()
        patch = patches[entry['id']] if patches is not None else json.loads((ROOT / entry['patch']).read_text())
        if entry['source-sha256'] != digest or patch['source-sha256'] != digest or patch['replacement'] != adapt(form['form'], form['name']):
            raise ValueError('whole source hash/body mismatch')
        if len(Scanner(patch['replacement']).all()) != 1: raise ValueError('complete one-form replacement')
    return len(IDS)

if __name__ == '__main__':
    print(f'{verify()} complete pinned runtime graph dependencies verified; native pending')
