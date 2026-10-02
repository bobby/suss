#!/usr/bin/env python3
"""Reproduce the bounded pinned apply generators without a JVM (bootstrap v1).

This development adaptation is not full compiled macro bootstrap acceptance.
Preserve the complete generator source/EPL; check its exact pin and hashes.
"""
import hashlib
import json
import sys
from pathlib import Path
from core_import import ROOT, pinned_file, extract_forms
from cljs_inventory import PIN

BEGIN = ';; BEGIN pinned apply bootstrap v1\n'
END = ';; END pinned apply bootstrap v1\n'


def apply_to(n=1):
    if n > 20:
        return '(throw (suss.bootstrap/error "Only up to 20 arguments supported on functions"))'
    return f'(let [a{n-1} (-first args) args (-rest args)] (if (== argc {n}) (f {" ".join(f"a{i}" for i in range(n))}) {apply_to(n+1)}))'


def simple(n=4):
    args = ' '.join(f'a{i}' for i in range(n+1))
    property_name = f'cljs$core$IFn$_invoke$arity${n+1}'
    invoke = f'(if (.-{property_name} f) (.{property_name} f {args}) (.call f f {args}))'
    more = f'(let [arr (array {args})] (loop [s next_{n}] (when s (do (.push arr (-first s)) (recur (next s))))) (.apply f f arr))' if n >= 19 else simple(n+1)
    source = 'args' if n == 4 else f'next_{n-1}'
    return f'(let [a{n} (cljs.core/-first {source}) next_{n} (cljs.core/next {source})] (if (nil? next_{n}) {invoke} {more}))'


def verify_generators():
    record = json.loads((ROOT / 'docs/compatibility/bootstrap/apply-generator-provenance.json').read_text())
    source = pinned_file(ROOT, record['source']).decode()
    if record['schema'] != 1 or record['upstream-commit'] != PIN or hashlib.sha256(source.encode()).hexdigest() != record['source-sha256']:
        raise ValueError('apply bootstrap pin/source hash mismatch')
    forms = extract_forms(source, 'macro', record['source'])
    originals = []
    for selected in record['forms']:
        original = forms[selected['id']]['form']
        if hashlib.sha256(original.encode()).hexdigest() != selected['sha256']:
            raise ValueError('apply generator source hash mismatch')
        originals.append(original)
    expected = source.split('\n\n', 1)[0] + '\n\n' + '\n\n'.join(originals) + '\n'
    if (ROOT / 'docs/compatibility/bootstrap/apply-generators.cljc').read_text() != expected:
        raise ValueError('apply generator extraction/notice mismatch')


def build(check=False):
    verify_generators()
    before_path = ROOT / 'docs/compatibility/bootstrap/sequence-forward-declarations.sus'
    before = before_path.read_text()
    definition = '(def apply-to (fn [f argc args] (let [args (seq args)] (if (zero? argc) (f) ' + apply_to() + '))))'
    block = BEGIN + '(declare apply-to)\n' + END
    if BEGIN in before:
        start = before.index(BEGIN); end = before.index(END, start) + len(END)
        expected = before[:start] + block + before[end:]
    else:
        expected = before + '\n' + block
    patch_path = ROOT / 'docs/compatibility/patches/apply-apply-to-simple.json'
    patch = json.loads(patch_path.read_text())
    original = extract_forms(pinned_file(ROOT, 'src/main/cljs/cljs/core.cljs').decode(), 'runtime', 'src/main/cljs/cljs/core.cljs')['runtime:apply-to-simple:4066']['form']
    # Explicit defn bootstrap only; retain metadata/docstring and every arity.
    replacement = original.replace('(defn- apply-to-simple', '(def ^:private apply-to-simple', 1)
    body_start = replacement.index('  ([f ^seq args]')
    replacement = replacement[:body_start] + '  (do ' + definition + '\n (fn\n' + replacement[body_start:] + '))'
    replacement = replacement.replace('(gen-apply-to-simple f 4 args)', simple())
    patch['replacement'] = replacement
    encoded = json.dumps(patch, indent=2) + '\n'
    if check:
        if before != expected or patch_path.read_text() != encoded:
            raise ValueError('stale apply bootstrap; run python3 scripts/apply_bootstrap.py')
    else:
        before_path.write_text(expected)
        patch_path.write_text(encoded)
        recipe_path = ROOT / 'docs/compatibility/core-import.json'
        recipe = json.loads(recipe_path.read_text())
        recipe['loader']['before']['sha256'] = hashlib.sha256(expected.encode()).hexdigest()
        recipe_path.write_text(json.dumps(recipe, indent=2) + '\n')


if __name__ == '__main__':
    if sys.argv[1:] not in ([], ['--check']):
        raise SystemExit('usage: apply_bootstrap.py [--check]')
    build(check=bool(sys.argv[1:]))
    print('bounded apply bootstrap v1 and four complete licensed generators verified')
