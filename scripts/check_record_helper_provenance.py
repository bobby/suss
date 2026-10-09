#!/usr/bin/env python3
"""Check whole helper adaptation bodies, independently of execution receipts."""
import hashlib
import json
from pathlib import Path
from core_import import ROOT, extract_forms, pinned_file, PIN
from cljs_inventory import Scanner

IDENTITIES = ['runtime:vary-meta:4171', 'runtime:fnil:4526', 'runtime:update-in:5530', 'runtime:group-by:11244']

def replacement(raw, name):
    tree = Scanner(raw).all()[0]
    children = tree.children
    if children[0].text != 'defn' or children[1].text != name or children[2].kind != 'string':
        raise ValueError('source declaration shape')
    start = children[3].start
    body = raw[start:tree.end - 1]
    if name == 'fnil':
        positions = []
        def walk(node):
            if node.kind == 'atom' and node.text == 'nil?':
                positions.append((node.start, node.end))
            for child in node.children:
                walk(child)
        for node in children[3:]: walk(node)
        for first, last in reversed(positions):
            body = body[:first - start] + 'suss.bootstrap/nil?' + body[last - start:]
    return '(def ' + name + '\n  ' + raw[children[2].start:children[2].end] + '\n  (fn ' + body + '))'

def verify():
    path = 'src/main/cljs/cljs/core.cljs'
    forms = extract_forms(pinned_file(ROOT, path).decode(), 'runtime', 'clojurescript/' + path)
    recipe = json.loads((ROOT / 'docs/compatibility/core-import.json').read_text())
    if recipe['upstream-commit'] != PIN or [entry['id'] for entry in recipe['forms'][-4:]] != IDENTITIES:
        raise ValueError('closed ordered helper selections')
    for entry in recipe['forms'][-4:]:
        source = forms[entry['id']]
        digest = hashlib.sha256(source['form'].encode()).hexdigest()
        patch = json.loads((ROOT / entry['patch']).read_text())
        if entry['source-sha256'] != digest or patch['source-sha256'] != digest or patch['replacement'] != replacement(source['form'], source['name']):
            raise ValueError('complete declaration/body adaptation mismatch')
    print('Verified four complete pinned helper adaptations; native execution not established')

if __name__ == '__main__':
    verify()
