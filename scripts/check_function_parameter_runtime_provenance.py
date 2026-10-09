#!/usr/bin/env python3
"""Exact whole runtime dependencies for parameter lowering; no execution inference."""
import hashlib
import json
from core_import import ROOT, PIN, extract_forms, pinned_file
from cljs_inventory import Scanner
IDS = ('runtime:LITE_MODE:56', 'runtime:to-array:3819',
       'runtime:--destructure-map:4154', 'runtime:reverse:3367',
       'runtime:vector:5959', 'runtime:some:4430')

def adapt(raw, name):
    tree = Scanner(raw).all()[0]
    nodes = tree.children
    if name == 'LITE_MODE':
        if nodes[0].text != 'goog-define': raise ValueError('complete define shape')
        return raw[:nodes[0].start] + 'def' + raw[nodes[0].end:]
    if nodes[0].text != 'defn': raise ValueError('whole function declaration')
    doc = nodes[2].kind == 'string'
    body = raw[nodes[3 if doc else 2].start:tree.end-1]
    if name == 'to-array':
        if body.count('(nil? s)') != 1 or body.count('(. ary push (first s))') != 1: raise ValueError('exact primitive macro/dot sites')
        body = body.replace('(nil? s)', '(suss.bootstrap/nil? s)').replace('(. ary push (first s))', '(.push ary (first s))')
    if name == 'some':
        before = '(when-let [s (seq coll)]\n    (or (pred (first s)) (recur pred (next s))))'
        after = '(let [some_seq_once__ (seq coll)]\n    (when some_seq_once__\n      (let [s some_seq_once__]\n        (or (pred (first s)) (recur pred (next s))))))'
        if body.count(before) != 1: raise ValueError('exact complete when-let site')
        body = body.replace(before, after)
    return '(def ' + name + '\n' + ('  ' + raw[nodes[2].start:nodes[2].end] + '\n' if doc else '') + '  (fn ' + body + '))'

def verify(recipe=None, patches=None):
    if recipe is None: recipe = json.loads((ROOT / 'docs/compatibility/core-import.json').read_text())
    if recipe['upstream-commit'] != PIN or tuple(entry['id'] for entry in recipe['forms'][350:356]) != IDS:
        raise ValueError('whole ordered parameter dependency group')
    source = extract_forms(pinned_file(ROOT, 'src/main/cljs/cljs/core.cljs').decode(), 'runtime', 'clojurescript/src/main/cljs/cljs/core.cljs')
    for entry in recipe['forms'][350:356]:
        if sum(item['id'] == entry['id'] for item in recipe['forms']) != 1: raise ValueError('duplicate selection')
        form = source[entry['id']]
        digest = hashlib.sha256(form['form'].encode()).hexdigest()
        patch = patches[entry['id']] if patches is not None else json.loads((ROOT / entry['patch']).read_text())
        if entry['source-sha256'] != digest or patch['source-sha256'] != digest or patch['replacement'] != adapt(form['form'], form['name']):
            raise ValueError('whole source hash/body mismatch')
        if len(Scanner(patch['replacement']).all()) != 1: raise ValueError('one complete replacement')
    return len(IDS)
if __name__ == '__main__':
    print(f'{verify()} complete pinned parameter runtime dependencies verified; final native pending')
