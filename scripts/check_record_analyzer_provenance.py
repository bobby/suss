#!/usr/bin/env python3
"""Verify complete pinned dependency retention, without inferring execution."""
import hashlib
import json
import subprocess
from pathlib import Path
from cljs_inventory import PIN, Scanner, declarations, inventory_records

ROOT = Path(__file__).resolve().parents[1]
DIRECTORY = ROOT / 'docs/compatibility/record-analyzer-context'

def digest(raw):
    return hashlib.sha256(raw).hexdigest()

def expected_forms():
    inventory_records()  # Requires the actual clean pinned checkout.
    predicates = {
        'src/main/clojure/cljs/core.cljc': lambda name, line: 1444 <= line <= 1708 or name in {'fast-path-protocols', 'fast-path-protocol-partitions-count'},
        'src/main/clojure/cljs/analyzer.cljc': lambda name, line: name in {'resolve-var', 'resolve-existing-var', 'elide-reader-meta', 'parse-type', 'js-reserved'},
        'src/main/clojure/cljs/compiler.cljc': lambda name, line: name in {'munge', 'munge-reserved', 'shadow-depth', 'hash-scope', 'fn-self-name'},
        'src/main/cljs/cljs/core.cljs': lambda name, line: name in {'CHAR_MAP', 'munge-str', 'nil-iter', 'gensym', 'gensym_counter'},
    }
    dirty = subprocess.check_output(['git', '-C', str(ROOT / 'clojurescript'), 'status', '--porcelain', '--', *predicates], text=True)
    if dirty:
        raise ValueError('retained upstream dependencies have uncommitted changes')
    result = []
    for path, predicate in predicates.items():
        source = (ROOT / 'clojurescript' / path).read_text()
        for top in Scanner(source).all():
            for kind, name, form, context in declarations(top):
                line = source.count('\n', 0, form.start) + 1
                if predicate(name, line):
                    result.append((path, kind, name, form, list(context), source))
            text = source[top.start:top.end]
            if text.startswith('(core/defmethod extend-prefix') or text.startswith('(defmethod emit* :defrecord'):
                result.append((path, 'defmethod', 'anonymous source stanza', top, [], source))
    return result

def verify(provenance=None):
    if provenance is None:
        provenance = json.loads((DIRECTORY / 'provenance.json').read_text())
    if provenance['schema'] != 1 or provenance['upstream'] != PIN or provenance['license'] != 'EPL-1.0':
        raise ValueError('schema/pin/license mismatch')
    expected = expected_forms()
    entries = provenance['forms']
    if len(entries) != 44 or len(expected) != 44:
        raise ValueError('closed dependency inventory mismatch')
    retained_paths = set()
    for entry, (path, kind, name, form, context, source) in zip(entries, expected):
        text = source[form.start:form.end]
        identity = ('clojurescript/' + path, kind, name, source.count('\n', 0, form.start) + 1, source.count('\n', 0, form.end) + 1, context)
        actual = tuple(entry[key] for key in ('source', 'kind', 'name', 'line', 'end-line', 'reader-context'))
        if actual != identity:
            raise ValueError('ordered whole-declaration identity mismatch')
        retained = ROOT / entry['retained']
        if retained.parent != DIRECTORY or retained in retained_paths:
            raise ValueError('invalid or duplicate retained path')
        retained_paths.add(retained)
        raw = retained.read_bytes()
        if raw != (text + '\n').encode() or entry['sha256'] != digest(text.encode()) or entry['retained-file-sha256'] != digest(raw):
            raise ValueError('whole source retention/hash mismatch')
        if entry['status'] != 'whole source retained; compilation/execution not established':
            raise ValueError('unsupported execution status')
    expected_notice = (ROOT / 'clojurescript/src/main/clojure/cljs/core.cljc').read_text().split('\n\n')[0] + '\n'
    if (DIRECTORY / 'NOTICE').read_text() != expected_notice:
        raise ValueError('upstream notice mismatch')
    return len(entries)

if __name__ == '__main__':
    print(f'Verified {verify()} complete pinned dependency forms/stanzas; execution not established')
