#!/usr/bin/env python3
"""Verify standalone licensed sequence setup against complete pinned source forms."""
import hashlib
import json
from pathlib import Path
from cljs_inventory import PIN, ROOT, Scanner
from core_import import pinned_file, repository_file, exact, json_data

PROVENANCE = 'docs/compatibility/sequence-loader-provenance.json'


def verify_payload(record, source, loader):
    exact(record, 'schema upstream-commit source-file-sha256 loader statements', 'sequence provenance')
    if record['schema'] != 1 or record['upstream-commit'] != PIN:
        raise ValueError('unsupported sequence provenance pin/schema')
    if hashlib.sha256(source.encode()).hexdigest() != record['source-file-sha256']:
        raise ValueError('sequence source-file hash mismatch')
    original = {}
    for form in Scanner(source).all():
        bounds = (source.count('\n', 0, form.start) + 1,
                  source.count('\n', 0, form.end) + 1)
        original[bounds] = source[form.start:form.end]
    expected = []
    for statement in record['statements']:
        exact(statement, 'source line end-line sha256', 'sequence statement')
        if statement['source'] != 'clojurescript/src/main/cljs/cljs/core.cljs':
            raise ValueError('unexpected sequence source')
        text = original.get((statement['line'], statement['end-line']))
        if text is None or hashlib.sha256(text.encode()).hexdigest() != statement['sha256']:
            raise ValueError('sequence statement is not a complete matching source form')
        expected.append(text)
    actual = [loader[form.start:form.end] for form in Scanner(loader).all()]
    if actual != expected:
        raise ValueError('sequence loader differs from retained standalone forms')
    notice = source.split('\n\n', 1)[0]
    if 'Copyright' not in notice or 'Eclipse Public License' not in notice or not loader.startswith(notice + '\n'):
        raise ValueError('sequence loader must retain complete upstream notice')


def verify():
    record = json_data((ROOT / PROVENANCE).read_bytes())
    source = pinned_file(ROOT, 'src/main/cljs/cljs/core.cljs').decode()
    loader = repository_file(ROOT, record['loader']).read_text()
    verify_payload(record, source, loader)
    return len(record['statements'])


if __name__ == '__main__':
    print(f'{verify()} complete licensed sequence setup forms verified')
