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
        required = 'source line end-line sha256'
        exact(statement, required + (' patch patch-sha256' if 'patch' in statement else ''), 'sequence statement')
        if statement['source'] != 'clojurescript/src/main/cljs/cljs/core.cljs':
            raise ValueError('unexpected sequence source')
        text = original.get((statement['line'], statement['end-line']))
        if text is None or hashlib.sha256(text.encode()).hexdigest() != statement['sha256']:
            raise ValueError('sequence statement is not a complete matching source form')
        if 'patch' in statement:
            patch_bytes = repository_file(ROOT, statement['patch']).read_bytes()
            if hashlib.sha256(patch_bytes).hexdigest() != statement['patch-sha256']:
                raise ValueError('sequence setup patch file hash mismatch')
            patch = json_data(patch_bytes)
            exact(patch, 'schema source-sha256 replacement rationale', 'sequence setup patch')
            if type(patch['schema']) is not int or patch['schema'] != 1:
                raise ValueError('unsupported sequence setup patch schema')
            if patch['source-sha256'] != statement['sha256']:
                raise ValueError('sequence setup patch source hash is stale')
            if not isinstance(patch['rationale'], str) or not patch['rationale'].strip():
                raise ValueError('sequence setup patch rationale is required')
            replacement = patch['replacement']
            if not isinstance(replacement, str):
                raise ValueError('sequence setup replacement must be source text')
            forms = Scanner(replacement).all()
            # Standalone adaptations preserve the form head and target; named
            # declarations are instead checked by core_import.adapt_form.
            originals = Scanner(text).all()
            original_target = [text[x.start:x.end] for x in originals[0].children[:2]]
            # Exact pinned compiler property munging for canonical collection roots.
            # No other target rewriting is permitted by the loader verifier.
            if original_target == ['set!', '(.-EMPTY-NODE PersistentVector)']:
                original_target[1] = '(.-EMPTY_NODE PersistentVector)'
            if original_target == ['set!', '(.-HASHMAP-THRESHOLD PersistentArrayMap)']:
                original_target[1] = '(.-HASHMAP_THRESHOLD PersistentArrayMap)'
            if original_target == ['extend-protocol', 'IPrintWithWriter']:
                # Bootstrap expansion may install the stanzas whose source types
                # are loaded. Preserve their order, targets and exact methods;
                # never accept invented types, rewritten bodies or duplicate
                # dispatch entries through this expansion path.
                upstream = originals[0].children
                stanzas = [(text[t.start:t.end], text[m.start:m.end])
                           for t, m in zip(upstream[2::2], upstream[3::2])]
                if (len(forms) != 1 or forms[0].kind != 'list'
                        or len(forms[0].children) < 2
                        or replacement[forms[0].children[0].start:forms[0].children[0].end] != 'do'
                        ):
                    raise ValueError('printer extension requires ordered exact source stanzas')
                previous = -1
                for extension in forms[0].children[1:]:
                    children = extension.children
                    if (extension.kind != 'list' or len(children) != 4
                            or replacement[children[0].start:children[0].end] != 'extend-type'
                            or replacement[children[2].start:children[2].end] != 'IPrintWithWriter'):
                        raise ValueError('printer extension requires ordered exact source stanzas')
                    stanza = (replacement[children[1].start:children[1].end],
                              replacement[children[3].start:children[3].end])
                    if stanza not in stanzas or stanzas.index(stanza) <= previous:
                        raise ValueError('printer extension requires ordered exact source stanzas')
                    previous = stanzas.index(stanza)
            elif (len(forms) != 1 or forms[0].kind != 'list'
                    or len(forms[0].children) < 2
                    or [replacement[x.start:x.end] for x in forms[0].children[:2]]
                    != original_target):
                raise ValueError('sequence setup patch must preserve one complete form and target')
            text = replacement
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
