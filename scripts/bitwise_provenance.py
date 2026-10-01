#!/usr/bin/env python3
"""Verify the conditional imul source region omitted by the top-level inventory."""
from core_import import ROOT, PIN, Scanner, digest, exact, json_data, pinned_file, repository_file
from cljs_inventory import bare

DOCUMENT = 'docs/compatibility/bitwise-primitives.json'
SOURCE = 'clojurescript/src/main/cljs/cljs/core.cljs'


def verify(document, source):
    exact(document, 'schema upstream source source-file-sha256 conditional-imul', 'bitwise provenance')
    if type(document['schema']) is not int or document['schema'] != 1:
        raise ValueError('invalid bitwise provenance schema')
    if document['upstream'] != PIN or document['source'] != SOURCE:
        raise ValueError('incorrect bitwise provenance pin/source')
    if document['source-file-sha256'] != digest(source):
        raise ValueError('stale bitwise source file hash')
    region = exact(document['conditional-imul'],
                   'line end-line source-region-sha256 adaptation rationale', 'conditional imul')
    start, end = region['line'], region['end-line']
    lines = source.splitlines(keepends=True)
    if type(start) is not int or type(end) is not int or not 1 <= start <= end <= len(lines):
        raise ValueError('invalid conditional imul line range')
    raw = b''.join(lines[start - 1:end])
    if digest(raw) != region['source-region-sha256']:
        raise ValueError('stale conditional imul region hash')
    text = raw.decode('utf-8')
    forms = Scanner(text).all()
    if (len(forms) != 1 or forms[0].start != 0 or forms[0].end != len(text.rstrip())
            or forms[0].kind != 'list' or forms[0].children[0].text != 'if'):
        raise ValueError('conditional imul region must contain exactly the complete if form')
    conditional = forms[0]
    if (len(conditional.children) != 4
            or any(branch.kind != 'list' or len(branch.children) != 4
                   or bare(branch.children[0]).text != 'defn'
                   or bare(branch.children[1]).text != 'imul'
                   or branch.children[2].kind != 'vector'
                   or [item.text for item in branch.children[2].children] != ['a', 'b']
                   for branch in conditional.children[2:])):
        raise ValueError('conditional region must define both pinned imul branches')
    if region['adaptation'] != 'crates/suss-compile/src/runtime_abi/bitwise.rs':
        raise ValueError('incorrect conditional imul adaptation')
    if not isinstance(region['rationale'], str) or not region['rationale'].strip():
        raise ValueError('missing conditional imul rationale')


def check(root=ROOT):
    document = json_data(repository_file(root, DOCUMENT).read_text())
    source = pinned_file(root, SOURCE.removeprefix('clojurescript/'))
    verify(document, source)
    repository_file(root, document['conditional-imul']['adaptation']).read_bytes()
    return document, source


if __name__ == '__main__':
    check()
    print('conditional imul pin, file hash and complete source region verified')
