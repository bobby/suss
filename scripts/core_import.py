#!/usr/bin/env python3
"""Reproduce reviewed upstream forms, explicit patches and EPL packaging.

This development tool does not execute upstream source or certify semantics.
The generated manifest is evidence of bytes/provenance, not compatibility.
"""
import argparse
import hashlib
import json
import subprocess
from pathlib import Path

from cljs_inventory import PIN, ROOT, SOURCES, Scanner, declarations, generate, inventory_records
from cljs_reviews import data, fields, keyword, validate

RECIPE = 'docs/compatibility/core-import.json'
DESTINATION = 'runtime/core-import'
VERSION = 1


def json_data(raw):
    def unique(pairs):
        result = {}
        for key, value in pairs:
            if key in result:
                raise ValueError(f'duplicate JSON key: {key}')
            result[key] = value
        return result
    return json.loads(raw, object_pairs_hook=unique)


def pinned_file(root, relative):
    actual = repository_file(root, 'clojurescript/' + relative).read_bytes()
    expected = subprocess.check_output(['git', '-C', str(root / 'clojurescript'),
                                        'show', f'{PIN}:{relative}'])
    if actual != expected:
        raise ValueError(f'upstream file differs from pin: {relative}')
    return actual


def digest(value):
    return hashlib.sha256(value).hexdigest()


def exact(value, names, label):
    if not isinstance(value, dict) or set(value) != set(names.split()):
        raise ValueError(f'{label} requires exactly {names}')
    return value


def extract_forms(source, phase, path):
    result = {}
    for top in Scanner(source).all():
        for kind, name, form, context in declarations(top):
            line = source.count('\n', 0, form.start) + 1
            identity = f'{phase}:{name}:{line}'
            if identity in result:
                raise ValueError(f'duplicate declaration ID: {identity}')
            result[identity] = {
                'id': identity, 'name': name, 'kind': kind, 'phase': phase,
                'source': path, 'line': line,
                'end-line': source.count('\n', 0, form.end) + 1,
                'byte-start': len(source[:form.start].encode()),
                'byte-end': len(source[:form.end].encode()),
                'context': list(context), 'form': source[form.start:form.end],
            }
    return result


def adapt_form(original, name, patch):
    exact(patch, 'schema source-sha256 replacement rationale', 'patch')
    if type(patch['schema']) is not int or patch['schema'] != VERSION:
        raise ValueError('unsupported patch schema')
    if patch['source-sha256'] != digest(original.encode()):
        raise ValueError('patch source hash is stale')
    if not isinstance(patch['rationale'], str) or not patch['rationale'].strip():
        raise ValueError('patch rationale is required')
    replacement = patch['replacement']
    if not isinstance(replacement, str):
        raise ValueError('patch replacement must be source text')
    forms = Scanner(replacement).all()
    if len(forms) != 1 or forms[0].kind != 'list':
        raise ValueError('patch must replace exactly one declaration form')
    decls = list(declarations(forms[0]))
    if len(decls) != 1 or decls[0][1] != name or decls[0][2] is not forms[0]:
        raise ValueError('patch must preserve the declaration identity')
    return replacement


def repository_file(root, path):
    if not isinstance(path, str) or not path or Path(path).is_absolute():
        raise ValueError('recipe paths must be relative repository files')
    target = (root / path).resolve()
    if not target.is_relative_to(root.resolve()) or not target.is_file():
        raise ValueError(f'missing or escaping repository file: {path}')
    return target


def build(root=ROOT):
    # inventory_records validates the pin and clean source before any extraction.
    if root != ROOT:
        raise ValueError('core import must use this checkout')
    records = inventory_records()
    generated, _ = generate()
    inventory_path = root / 'docs/compatibility/cljs-core.edn'
    if inventory_path.read_text() != generated:
        raise ValueError('tracked inventory is stale')
    overlay_path = root / 'docs/compatibility/reviews.edn'
    validate(overlay_path.read_text(), records)
    overlay = fields(data(Scanner(overlay_path.read_text()).all()[0]),
                     'schema upstream-commit reviews', 'overlay')['reviews']
    recipe_path = root / RECIPE
    recipe = exact(json_data(recipe_path.read_bytes()),
                   'schema upstream-commit namespace phase forms', 'recipe')
    if type(recipe['schema']) is not int or recipe['schema'] != VERSION or recipe['upstream-commit'] != PIN:
        raise ValueError('unsupported recipe schema or source pin')
    if recipe['namespace'] != 'suss.core' or recipe['phase'] not in ('runtime', 'macro'):
        raise ValueError('recipe requires canonical suss.core and an explicit phase')
    if not isinstance(recipe['forms'], list) or not recipe['forms']:
        raise ValueError('recipe requires selected forms in explicit dependency order')
    sources, forms, notices = {}, {}, {}
    for relative in SOURCES:
        path = 'clojurescript/' + relative
        raw = pinned_file(root, relative)
        source = raw.decode('utf-8')
        phase = 'runtime' if relative.endswith('.cljs') else 'macro'
        forms.update(extract_forms(source, phase, path))
        sources[path] = digest(raw)
        notice = source.split('\n\n', 1)[0]
        if not notice.startswith(';') or 'Copyright' not in notice or 'Eclipse Public License' not in notice:
            raise ValueError(f'missing upstream notice: {path}')
        notices[path] = notice + '\n'
    outputs, entries, seen = {}, [], set()
    selected_notices = []
    adapted_forms = []
    for selection in recipe['forms']:
        exact(selection, 'id source-sha256 patch', 'selection')
        identity = selection['id']
        if not isinstance(identity, str) or identity in seen or identity not in forms:
            raise ValueError(f'unknown or duplicate selected ID: {identity}')
        seen.add(identity)
        if identity not in overlay:
            raise ValueError(f'unreviewed declaration: {identity}')
        form = forms[identity]
        review = overlay[identity]
        # Inventory context is provenance, not executable wrapping. Flattening
        # a reader branch changes feature selection; flattening let/binding loses
        # lexical/dynamic bindings. Reject until import preserves those semantics.
        if form['context']:
            raise ValueError(f'unsupported declaration context for {identity}: {form["context"]}')
        if form['phase'] != recipe['phase']:
            raise ValueError('phase mismatch in selected form')
        raw = form['form'].encode()
        original_hash = digest(raw)
        if selection['source-sha256'] != original_hash:
            raise ValueError(f'stale extraction hash: {identity}')
        classification = review[keyword('classification')]
        if review[keyword('status')] == keyword('excluded') or classification == keyword('host-specific'):
            raise ValueError('cannot import an excluded or host-specific form')
        patch_path = selection['patch']
        if patch_path is None:
            if classification != keyword('portable'):
                raise ValueError('adapted declaration requires an explicit patch')
            adapted = form['form']
            patch_hash = None
        else:
            if classification != keyword('adapted') or review[keyword('adaptation-path')] != patch_path:
                raise ValueError('patch must match the reviewed adaptation path')
            patch_bytes = repository_file(root, patch_path).read_bytes()
            adapted = adapt_form(form['form'], form['name'], json_data(patch_bytes))
            patch_hash = digest(patch_bytes)
        notice = notices[form['source']]
        if notice not in selected_notices:
            selected_notices.append(notice)
        stem = f'{len(entries):04d}'
        extracted_path = f'extracted/{stem}.cljs' if form['phase'] == 'runtime' else f'extracted/{stem}.cljc'
        outputs[extracted_path] = (notice + '\n' + form['form'] + '\n').encode()
        adapted_forms.append(adapted)
        entries.append({key: value for key, value in form.items() if key != 'form'} | {
            'source-file-sha256': sources[form['source']], 'source-sha256': original_hash,
            'extracted': extracted_path, 'extracted-sha256': digest(outputs[extracted_path]),
            'patch': patch_path, 'patch-sha256': patch_hash,
            'adapted-form-sha256': digest(adapted.encode()),
            'notice-sha256': digest(notice.encode()),
            'review-classification': classification.name,
            'review-status': review[keyword('status')].name,
            'dependencies': review[keyword('dependencies')],
            'semantic-tests': review[keyword('tests')],
        })
    extension = 'sus' if recipe['phase'] == 'runtime' else 'cljc'
    artifact = f'suss/core.{extension}'
    output = '\n'.join(selected_notices) + '\n;; Generated reviewed core import; see ../manifest.json.\n'
    output += f'(ns {recipe["namespace"]})\n\n' + '\n\n'.join(adapted_forms) + '\n'
    outputs[artifact] = output.encode()
    for name in ('LICENSE', 'epl-v10.html'):
        outputs[name] = pinned_file(root, name)
    manifest = {
        'schema': VERSION, 'upstream-commit': PIN, 'namespace': recipe['namespace'],
        'phase': recipe['phase'], 'tool': 'scripts/core_import.py',
        'tool-sha256': digest(Path(__file__).read_bytes()),
        'scanner-sha256': digest((root / 'scripts/cljs_inventory.py').read_bytes()),
        'review-validator-sha256': digest((root / 'scripts/cljs_reviews.py').read_bytes()),
        'recipe': RECIPE, 'recipe-sha256': digest(recipe_path.read_bytes()),
        'reviews-sha256': digest(overlay_path.read_bytes()),
        'inventory-sha256': digest(inventory_path.read_bytes()), 'forms': entries,
        'files': {path: digest(value) for path, value in sorted(outputs.items())},
    }
    outputs['manifest.json'] = (json.dumps(manifest, indent=2, ensure_ascii=False) + '\n').encode()
    return outputs


def verify_outputs(destination, outputs):
    existing = {p.relative_to(destination).as_posix() for p in destination.rglob('*') if p.is_file()}
    extras = existing - outputs.keys()
    if extras:
        raise ValueError(f'unexpected generated files; inspect rather than delete: {sorted(extras)}')
    for path, value in outputs.items():
        target = destination / path
        if not target.is_file() or target.read_bytes() != value:
            raise ValueError(f'stale core import: {path}; run python3 scripts/core_import.py')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    outputs = build()
    destination = ROOT / DESTINATION
    existing = {p.relative_to(destination).as_posix() for p in destination.rglob('*') if p.is_file()}
    extras = existing - outputs.keys()
    if extras:
        raise SystemExit(f'unexpected generated files; inspect rather than delete: {sorted(extras)}')
    if args.check:
        verify_outputs(destination, outputs)
    else:
        for path, value in outputs.items():
            target = destination / path
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(value)
    print(f'{len(outputs)} core import files ' + ('verified' if args.check else 'written'))


if __name__ == '__main__':
    main()
