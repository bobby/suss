#!/usr/bin/env python3
"""Check retained when-let provenance; does not compile or execute Suss."""
import difflib
import hashlib
import json
import re
from pathlib import Path
import subprocess
import sys

sys.dont_write_bytecode = True
from cljs_inventory import Scanner

ROOT = Path(__file__).resolve().parents[1]
DIRECTORY = ROOT / 'docs/compatibility/when-let'


def digest(data):
    return hashlib.sha256(data).hexdigest()


def main():
    record = json.loads((DIRECTORY / 'provenance.json').read_text())
    actual = subprocess.check_output(
        ['git', '-C', str(ROOT / 'clojurescript'), 'rev-parse', 'HEAD'], text=True).strip()
    assert actual == record['upstream'], actual
    assert not subprocess.check_output(
        ['git', '-C', str(ROOT / 'clojurescript'), 'status', '--porcelain'], text=True)
    assert digest((ROOT / 'scripts/cljs_inventory.py').read_bytes()) == record['extraction_tool_sha256']
    for source in record['sources']:
        raw = (ROOT / 'clojurescript' / source['path']).read_bytes()
        assert digest(raw) == source['source_sha256'], source['path']
        text = raw.decode()
        scanner = Scanner(text)
        scanner.pos = sum(len(line) for line in text.splitlines(True)[:source['start_line'] - 1])
        form = scanner.read()
        extracted = text[form.start:form.end]
        assert text.count('\n', 0, form.end) + 1 == source['end_line']
        assert digest(extracted.encode()) == source['form_sha256']
        retained = (DIRECTORY / source['retained']).read_bytes()
        assert retained == (extracted + '\n').encode()
        assert digest(retained) == source['retained_sha256']
    adapted = (DIRECTORY / 'adapted.sus').read_text()
    rust = (ROOT / 'crates/suss-compile/src/portable_when_let.rs').read_text()
    assert rust.split('r#"', 1)[1].rsplit('"#;', 1)[0] == adapted
    assert digest(adapted.encode()) == record['adapted_sha256']
    original = (DIRECTORY / 'upstream-when-let.cljc').read_text()
    patch = ''.join(difflib.unified_diff(original.splitlines(True), adapted.splitlines(True),
                                       fromfile='upstream-when-let.cljc', tofile='adapted.sus'))
    assert (DIRECTORY / 'when-let.patch').read_text() == patch
    assert digest(patch.encode()) == record['patch_sha256']
    for path, expected in record['files'].items():
        assert digest((ROOT / path).read_bytes()) == expected, path
    for path, expected in record['oracle_files'].items():
        assert digest((DIRECTORY / path).read_bytes()) == expected, path
    for path, expected in record['direct_let_oracle_files'].items():
        assert digest((DIRECTORY / path).read_bytes()) == expected, path
    assert (DIRECTORY / 'epl-v10.html').read_bytes() == (ROOT / 'runtime/core-import/epl-v10.html').read_bytes()
    oracle = DIRECTORY / 'oracle'
    for kind in ['run', 'validation']:
        assert (oracle / f'{kind}1-observations.json').read_bytes() == (oracle / f'{kind}2-observations.json').read_bytes()
        for number in [1, 2]:
            receipt = json.loads((oracle / f'{kind}{number}-receipt.json').read_text())
            assert receipt['source_unchanged']
            assert len(receipt['steps']) == 2
            assert all(step['exit'] == 0 for step in receipt['steps'])
    assert (ROOT / 'tests/oracle/when-let-observations.json').read_bytes() == (oracle / 'run1-observations.json').read_bytes()
    assert (ROOT / 'tests/oracle/when-let-validation-observations.json').read_bytes() == (oracle / 'validation1-observations.json').read_bytes()
    positive_inputs = json.loads((oracle / 'provenance-inputs.json').read_text())
    positive_fixture = 'crates/suss-cli/tests/compiled_when_let.rs'
    assert record['files'][positive_fixture] == positive_inputs['files'][positive_fixture]
    positive_guest = re.findall(r'observe\(&mut session, &mut macros, r#"(.*?)"#\)',
                                (ROOT / positive_fixture).read_text(), re.S)[0]
    assert (oracle / 'guest.cljs').read_text() == positive_guest
    direct = DIRECTORY / 'direct-let-oracle'
    direct_inputs = json.loads((direct / 'inputs.json').read_text())
    fixture = direct_inputs['native_fixture']
    assert record['files'][fixture] == direct_inputs['native_fixture_sha256']
    guests = re.findall(r'eval_with_macros\(r#"(.*?)"#, &mut macros\)',
                        (ROOT / fixture).read_text(), re.S)
    assert len(guests) == 2
    for index, guest in enumerate(guests, 1):
        path = f'guest{index}.cljs'
        assert (direct / path).read_text() == guest
        assert digest(guest.encode()) == direct_inputs['guests'][path]
    expected = json.loads((direct / 'expected.json').read_text())
    assert expected['schema'] == 1 and expected['upstream'] == record['upstream']
    for number in [1, 2]:
        receipt = json.loads((direct / f'run{number}-receipt.json').read_text())
        assert receipt['source_unchanged'] and len(receipt['steps']) == 2
        assert all(step['exit'] == 0 for step in receipt['steps'])
        assert json.loads((direct / f'run{number}-observations.json').read_text()) == expected
    print('when-let source provenance, adaptation, retained oracle bytes and receipts match; no Suss execution implied')


if __name__ == '__main__':
    main()
