#!/usr/bin/env python3
"""Vendored jank-lang/clojure-test-suite: source lock and development oracle.

The vendored files are byte-for-byte upstream content at a pinned commit. Local
adaptations live only in separate MPL-2.0 patch files and are applied to
generated copies; vendored bytes are never edited in place.

`check` (the default) verifies the lock offline. `oracle` regenerates the pinned
ClojureScript observations with Java/Node (a development-only dependency) and
fails unless they equal the reviewed reference; `--write` records them instead.
"""
import argparse
import collections
import hashlib
import io
import json
import os
import re
import shutil
import subprocess
import tarfile
from pathlib import Path

import oracle_compare

ROOT = Path(__file__).resolve().parents[1]
VENDOR = ROOT / 'vendor/clojure-test-suite'
UPSTREAM = VENDOR / 'upstream'
LOCK = ROOT / 'docs/compatibility/clojure-test-suite-lock.json'
REPOSITORY = 'https://github.com/jank-lang/clojure-test-suite'
COMMIT = '95d4a9112cfc5fe6629be3d14bb99080ea00af2f'
PATHS = ('LICENSE', 'README.md', 'test')
CLJS_PIN = 'c4295f303100bbf5afac449242d30bca1126f1a1'
ORACLE = ROOT / 'tests/oracle'
GENERATED = ORACLE / 'out/generated'
HARNESS = ROOT / 'tests/clojure-test-suite'
HOST_INTEROP = ROOT / 'docs/compatibility/clojure-test-suite-host-interop.json'
LEGACY_CASES = ROOT / 'docs/compatibility/cases.json'
LEGACY_OVERLAP = ROOT / 'docs/compatibility/clojure-test-suite-legacy-overlap.json'
INTEROP_CLASSES = ('suss-branch', 'harness', 'host-specific')
# The real suite, and a small harness self-test in the same shape that Suss can
# already execute. Each has its own reviewed oracle reference and baseline.
SUITES = {
    'suite': {'sources': UPSTREAM / 'test', 'reference': ORACLE / 'clojure-test-suite-observations.json',
              'baseline': ORACLE / 'clojure-test-suite-known-failures.json'},
    'fixture': {'sources': HARNESS / 'fixture', 'reference': ORACLE / 'clojure-test-suite-fixture-observations.json',
                'baseline': ORACLE / 'clojure-test-suite-fixture-known-failures.json'},
}
ORACLE_PATCHES = ('0001-when-var-exists-macro-phase.patch',)
SUSS_PATCHES = ('0002-number-range-suss-branches.patch',)
PORTABILITY = 'clojure.core-test.portability'
# Math.random is seeded per run; assertions whose observations differ across
# these seeds are nondeterministic and keep only their kind and verdict.
SEEDS = (1, 2, 3, 4)
# Reviewed random-number-dependent tests (or single assertions). Their operand
# values depend on the host generator, so Suss is judged on its verdict only.
# Every seed-detected difference must fall inside this list.
RANDOMIZED = {
    'clojure.core-test.rand/test-rand': 'samples from rand',
    'clojure.core-test.rand-int/test-rand-int': 'samples from rand-int',
    'clojure.core-test.rand-nth/test-rand-nth': 'samples from rand-nth',
    'clojure.core-test.random-sample/test-random-sample': 'samples from random-sample',
    'clojure.core-test.random-uuid/test-random-uuid': 'values from random-uuid',
    'clojure.core-test.shuffle/test-shuffle': 'results of shuffle',
    'clojure.core-test.constantly/test-constantly#30': 'constant chosen by rand-int',
    'clojure.core-test.uuid-qmark/test-uuid?#4': 'value from random-uuid',
}


def sha256(data):
    return hashlib.sha256(data).hexdigest()


def manifest(root):
    files = {}
    for path in sorted(p for p in root.rglob('*') if p.is_file()):
        files[path.relative_to(root).as_posix()] = sha256(path.read_bytes())
    if not files:
        raise ValueError(f'no vendored files under {root}')
    return files


def lock_data(files):
    return {'schema': 1, 'repository': REPOSITORY, 'commit': COMMIT,
            'license': 'MPL-2.0', 'paths': list(PATHS), 'files': files}


def git_manifest(checkout):
    archive = subprocess.check_output(['git', '-C', str(checkout), 'archive', COMMIT, *PATHS])
    files = {}
    with tarfile.open(fileobj=io.BytesIO(archive)) as tar:
        for member in tar.getmembers():
            if member.isfile():
                files[member.name] = sha256(tar.extractfile(member).read())
    return dict(sorted(files.items()))


def verify(lock):
    if lock != lock_data(lock.get('files', {})):
        raise ValueError('clojure-test-suite lock metadata mismatch')
    actual = manifest(UPSTREAM)
    if actual != lock['files']:
        changed = sorted(set(actual.items()) ^ set(lock['files'].items()))
        raise ValueError(f'vendored clojure-test-suite differs from lock: {changed[:5]}; '
                         'review upstream changes, do not silently regenerate')


def interop_occurrences():
    """(file, line) of every `js/` reference outside comments in the vendored tests."""
    found = set()
    for path in sorted((UPSTREAM / 'test').rglob('*.cljc')):
        for number, line in enumerate(path.read_text(encoding='utf-8').splitlines(), 1):
            if 'js/' in line.split(';', 1)[0]:
                found.add((path.relative_to(UPSTREAM / 'test').as_posix(), number))
    return found


def verify_interop():
    """Every host-interop reference must carry a reviewed classification."""
    document = json.loads(HOST_INTEROP.read_text(), object_pairs_hook=unique)
    if document['schema'] != 1 or document['suite'] != COMMIT:
        raise ValueError('unexpected host-interop classification identity')
    classified = set()
    for entry in document['entries']:
        if entry['classification'] not in INTEROP_CLASSES or not entry['rationale'] or not entry['alternative']:
            raise ValueError(f"incomplete host-interop classification: {entry['file']}")
        for line in entry['lines']:
            if (entry['file'], line) in classified:
                raise ValueError(f"duplicate host-interop classification: {entry['file']}:{line}")
            classified.add((entry['file'], line))
    occurrences = interop_occurrences()
    if classified != occurrences:
        raise ValueError(f'host-interop classifications differ from vendored js/ references: '
                         f'unclassified {sorted(occurrences - classified)}, stale {sorted(classified - occurrences)}')
    return len(occurrences)


def suite_namespace(symbol):
    """Upstream's test file naming (doc/writing-tests.md) as a namespace segment:
    a leading `-` is `minus`, other characters map to words joined by `-`."""
    special = {'*': 'star', '+': 'plus', '!': 'bang', "'": 'squote', '?': 'qmark',
               '<': 'lt', '>': 'gt', '=': 'eq', '%': 'percent', '/': 'slash'}
    out = ''
    for index, char in enumerate(symbol):
        if char == '-' and index == 0:
            out += 'minus'
        elif char in special:
            out += ('-' if out and not out.endswith('-') else '') + special[char]
        else:
            out += char
    return out


def legacy_overlap():
    """Map each legacy conformance case to suite namespaces testing the core functions it calls."""
    available = set(namespaces())
    cases = json.loads(LEGACY_CASES.read_text(), object_pairs_hook=unique)
    mapping, covered, uncovered = {}, 0, 0
    for identity, (expression, _expected) in sorted(cases.items()):
        heads = sorted(set(re.findall(r'\((?!\s)([^\s()\[\]{}"#^@~`\']+)', expression)))
        suite_namespaces = sorted({f'clojure.core-test.{suite_namespace(h)}' for h in heads}
                                  & available)
        mapping[identity] = {'expression': expression, 'functions': heads, 'suite-namespaces': suite_namespaces}
        if suite_namespaces:
            covered += 1
        else:
            uncovered += 1
    return {'schema': 1, 'suite': COMMIT,
            'summary': {'cases': len(cases), 'with-suite-coverage': covered, 'legacy-only': uncovered},
            'cases': mapping}


def namespaces(sources=UPSTREAM / 'test'):
    """Test namespaces in deterministic file order, excluding the portability helper."""
    found = []
    for path in sorted(sources.rglob('*.cljc')):
        match = re.match(r'\(ns\s+([^\s()]+)', path.read_text(encoding='utf-8'))
        if not match:
            raise ValueError(f'missing ns form: {path}')
        relative = path.relative_to(sources).with_suffix('').as_posix()
        if match[1].replace('.', '/').replace('-', '_') != relative:
            raise ValueError(f'namespace does not match path: {match[1]} {path}')
        if match[1] != PORTABILITY:
            found.append(match[1])
    return found


def apply_patches(target, names):
    for name in names:
        patch = VENDOR / 'patches' / name
        subprocess.run(['patch', '-s', '-p1', '-d', str(target), '-i', str(patch)], check=True)


def generate_oracle(suite):
    """Write the patched suite (plus the fixture, if selected) and its loader under out/generated."""
    shutil.rmtree(GENERATED / 'clojure', ignore_errors=True)
    staging = GENERATED.parent / 'clojure-test-suite-patch'
    shutil.rmtree(staging, ignore_errors=True)
    shutil.copytree(UPSTREAM / 'test', staging / 'test')
    apply_patches(staging, ORACLE_PATCHES)
    shutil.copytree(staging / 'test' / 'clojure', GENERATED / 'clojure')
    shutil.rmtree(staging)
    sources = SUITES[suite]['sources']
    if sources != UPSTREAM / 'test':
        shutil.copytree(sources / 'clojure', GENERATED / 'clojure', dirs_exist_ok=True)
    names = namespaces(sources)
    package = GENERATED / 'suss_oracle'
    package.mkdir(parents=True, exist_ok=True)
    for stale in package.glob('clojure_test_suite_marker_*.cljs'):
        stale.unlink()
    requires = []
    for index, name in enumerate(names):
        marker = f'suss-oracle.clojure-test-suite-marker-{index:03d}'
        (package / f'clojure_test_suite_marker_{index:03d}.cljs').write_text(
            f'(ns {marker} (:require [suss-oracle.clojure-test-suite-skips :as skips]))\n'
            f'(skips/loading! {json.dumps(name)})\n')
        requires += [f'[{marker}]', f'[{name}]']
    (package / 'clojure_test_suite_namespaces.cljs').write_text(
        '(ns suss-oracle.clojure-test-suite-namespaces\n  (:require\n   '
        + '\n   '.join(requires) + '))\n'
        f'(def commit {json.dumps(COMMIT)})\n'
        f'(def namespaces [{" ".join(json.dumps(name) for name in names)}])\n')
    return names


def generate_suss_sources(suite, output):
    """Write the Suss harness source tree: the suite with its Suss patches (or
    the fixture), overlaid by the harness namespaces, which replace the upstream
    portability helpers with their Suss counterparts."""
    output = Path(output)
    if output.exists() and any(output.iterdir()):
        raise ValueError(f'Suss source output must be empty: {output}')
    sources = SUITES[suite]['sources']
    shutil.copytree(sources, output, dirs_exist_ok=True)
    if suite == 'suite':
        staging = output.parent / f'{output.name}-patch'
        shutil.copytree(UPSTREAM / 'test', staging / 'test')
        apply_patches(staging, SUSS_PATCHES)
        shutil.copytree(staging / 'test', output, dirs_exist_ok=True)
        shutil.rmtree(staging)
    shutil.copytree(HARNESS / 'suss', output, dirs_exist_ok=True)
    return output


def run_oracle(suite):
    """Compile with the pinned ClojureScript and execute once per seed in Node."""
    pin = subprocess.check_output(['git', '-C', str(ROOT / 'clojurescript'), 'rev-parse', 'HEAD'], text=True).strip()
    if pin != CLJS_PIN:
        raise SystemExit(f'wrong ClojureScript oracle pin: {pin}')
    if subprocess.check_output(['git', '-C', str(ROOT / 'clojurescript'), 'status', '--porcelain'], text=True):
        raise SystemExit('dirty ClojureScript oracle source')
    generate_oracle(suite)
    environment = dict(os.environ, CLJ_CONFIG='/tmp/suss-oracle-clojure-config',
                       CLJ_CACHE='/tmp/suss-oracle-clojure-cache')
    options = (f'{{:target :nodejs :output-to "out/clojure-test-suite-{suite}.js" '
               f':output-dir "out/clojure-test-suite-{suite}-cljs" :optimizations :none :source-map false '
               ':force true :cache-analysis false :warnings {:invalid-arithmetic false}}')
    subprocess.run(['clojure', '-Srepro', '-M', '-m', 'cljs.main', '-co', options,
                    '-c', 'suss-oracle.clojure-test-suite'], cwd=ORACLE, env=environment, check=True)
    runs = []
    for seed in SEEDS:
        output = ORACLE / f'out/clojure-test-suite-{suite}-seed-{seed}.json'
        subprocess.run(['node', f'out/clojure-test-suite-{suite}.js', str(output), str(seed)], cwd=ORACLE, check=True)
        runs.append(json.loads(output.read_text(), object_pairs_hook=unique))
    return runs


def unique(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError(f'duplicate JSON key: {key}')
        result[key] = value
    return result


OBSERVATION_KEYS = {'thrown': ('threw',), 'predicate': ('operands',), 'value': ('value',),
                    'error': ('thrown', 'message')}


def canonical_form(form):
    """Printed forms embed process-global auto-gensym counters (`p1__22266#`)."""
    return re.sub(r'__\d+', '__N', form)


def form_head(form):
    """Head symbol of a printed assertion form, or None for non-list forms."""
    match = re.match(r'\(([^\s()\[\]{}"]+)', form)
    return match[1] if match else None


def normalize(*runs, suite='suite'):
    """Merge seeded runs into the reviewed reference, flagging nondeterministic assertions."""
    first = runs[0]
    for run in runs:
        if run.get('schema') != 1 or run.get('suite') != COMMIT or run.get('upstream') != CLJS_PIN:
            raise ValueError('unexpected oracle run identity')
        if run['reporter-errors']:
            raise ValueError(f"oracle reporter failed: {run['reporter-errors'][:3]}")
        summary = run['summary']
        if summary is None or summary['test'] != len(run['tests']) or \
           summary['pass'] + summary['fail'] + summary['error'] != len(run['assertions']):
            raise ValueError(f'oracle run is incomplete: {summary}')
        if summary['fail'] or summary['error']:
            raise ValueError(f'the oracle must pass every assertion it records: {summary}')
        if [run[k] for k in ('namespaces', 'tests', 'skips')] != [first[k] for k in ('namespaces', 'tests', 'skips')]:
            raise ValueError('oracle runs disagree on namespaces, tests or skips')
        if [(a['test'], a['ordinal'], a['kind'], canonical_form(a['form'])) for a in run['assertions']] != \
           [(a['test'], a['ordinal'], a['kind'], canonical_form(a['form'])) for a in first['assertions']]:
            raise ValueError('oracle runs disagree on assertion order')
    assertions, seen = [], set()
    for variants in zip(*(run['assertions'] for run in runs)):
        left = variants[0]
        identity = f"{left['test']}#{left['ordinal']}"
        if left['test'] is None or identity in seen:
            raise ValueError(f'assertion without a unique test identity: {identity}')
        seen.add(identity)
        keys = OBSERVATION_KEYS[left['kind']]
        stable = all(other[k] == left[k] for other in variants for k in keys + ('verdict',))
        randomized = identity in RANDOMIZED or left['test'] in RANDOMIZED
        if not stable and not randomized:
            raise ValueError(f'unreviewed nondeterministic assertion: {identity} {left["form"]}')
        deterministic = stable and not randomized
        entry = {'id': identity, 'test': left['test'], 'ordinal': left['ordinal'], 'kind': left['kind'],
                 'verdict': left['verdict'], 'deterministic': deterministic,
                 'form': canonical_form(left['form']), 'line': left['line'], 'column': left['column'],
                 'contexts': left['contexts']}
        if deterministic:
            entry.update({k: left[k] for k in keys})
        assertions.append(entry)
    unused = set(RANDOMIZED) - seen - {a['test'] for a in assertions}
    if suite == 'suite' and unused:
        raise ValueError(f'stale randomized entries: {sorted(unused)}')
    return {'schema': 1, 'suite': COMMIT, 'upstream': CLJS_PIN,
            'namespaces': first['namespaces'], 'tests': first['tests'], 'skips': first['skips'],
            'assertions': assertions, 'summary': first['summary']}


def summarize(reference):
    counts = {}
    for entry in reference['assertions']:
        key = (entry['verdict'], entry['kind'], entry['deterministic'])
        counts[key] = counts.get(key, 0) + 1
    return counts


def opaque(node):
    """True when a tagged value contains a node the oracle could not represent."""
    if node['tag'] == 'opaque':
        return True
    children = {'map': lambda n: [x for entry in n['entries'] for x in entry],
                'set': lambda n: n['items'], 'vector': lambda n: n['items'], 'seq': lambda n: n['items'],
                'keyword': lambda n: [n['namespace'], n['name']], 'symbol': lambda n: [n['namespace'], n['name']],
                'exception-info': lambda n: [n['data'], n['message'], n['cause']]}.get(node['tag'])
    return bool(children) and any(opaque(child) for child in children(node))


def decide(expected, actual):
    """Return (reason or None, guest_judged) for one oracle assertion and its Suss observation."""
    if expected['kind'] == 'value' and actual['kind'] == 'predicate':
        # cljs.test reports a call through a local or non-function var by its
        # result; the harness evaluates the same call and keeps that result.
        actual = dict(actual, kind='value', value=actual['result'])
    if actual['kind'] != expected['kind']:
        return f"kind:{actual['kind']}", False
    if actual.get('head') != form_head(expected['form']):
        return f"head:{actual.get('head')}", False
    if actual['verdict'] != expected['verdict']:
        return f"verdict:{actual['verdict']}", False
    if not expected['deterministic']:
        return None, True
    if expected['kind'] == 'thrown':
        return (None if actual['threw'] == expected['threw'] else 'observation:threw'), False
    if expected['kind'] == 'value':
        pairs = [(expected['value'], actual['value'])]
    elif expected['kind'] == 'predicate':
        if len(actual['operands']) != len(expected['operands']):
            return 'observation:arity', False
        pairs = list(zip(expected['operands'], actual['operands']))
    else:
        return 'observation:error', False
    judged = False
    for oracle_value, suss_value in pairs:
        if opaque(oracle_value):
            judged = True
        elif opaque(suss_value):
            return 'observation:undecodable', False
        elif not oracle_compare.matches(oracle_value, suss_value):
            return 'observation:value', False
    return None, judged


def compare(reference, observed):
    """Decide every oracle assertion from Suss observations; return counts and failures.

    Counts: `pass` (decided host-side), `guest-judged` (verdict only: randomized
    or opaque oracle operands), `fail` (executed with a wrong kind, head,
    verdict or observation; a test whose assertion count differs from the
    oracle's; unexpected assertions; skip mismatches) and `not-executed`.
    """
    if observed.get('schema') != 1 or observed.get('suite') != COMMIT:
        raise ValueError('unexpected Suss observation identity')
    if [n['namespace'] for n in observed['namespaces']] != reference['namespaces']:
        raise ValueError('Suss observations must cover every suite namespace in order')
    namespace_failures = {n['namespace']: {'stage': n['stage'], 'diagnostic': n['diagnostic']}
                          for n in observed['namespaces'] if n['status'] != 'loaded'}
    test_failures = {t['test']: {'stage': t['stage'], 'diagnostic': t['diagnostic']}
                     for t in observed['test-failures']}
    actual, per_test = {}, collections.Counter()
    for entry in observed['assertions']:
        identity = f"{entry['test']}#{entry['ordinal']}"
        if identity in actual:
            raise ValueError(f'duplicate Suss assertion: {identity}')
        actual[identity] = entry
        per_test[entry['test']] += 1
    expected_per_test = collections.Counter(entry['test'] for entry in reference['assertions'])
    failures = {}
    counts = {'pass': 0, 'guest-judged': 0, 'fail': 0, 'not-executed': 0}
    for entry in reference['assertions']:
        test = entry['test']
        namespace = test.split('/')[0]
        found = actual.get(entry['id'])
        if namespace in namespace_failures:
            reason = f"namespace:{namespace_failures[namespace]['stage']}"
        elif test in test_failures and found is None:
            reason = f"test:{test_failures[test]['stage']}"
        elif per_test[test] != expected_per_test[test] and per_test[test]:
            # Ordinals no longer align once control flow diverges; no assertion
            # of this test may pass by coincidence against a shifted record.
            reason = f'count:{per_test[test]}/{expected_per_test[test]}'
        elif found is None:
            reason = 'missing'
        else:
            reason, judged = decide(entry, found)
            if not reason:
                counts['guest-judged' if judged else 'pass'] += 1
                continue
        if reason.startswith(('namespace:', 'test:', 'missing')):
            counts['not-executed'] += 1
        else:
            counts['fail'] += 1
        # A failed namespace's entry already accounts for all of its assertions.
        if namespace not in namespace_failures:
            failures[entry['id']] = reason
    for identity in sorted(set(actual) - {entry['id'] for entry in reference['assertions']}):
        failures[identity] = 'unexpected-assertion'
        counts['fail'] += 1
    skip = lambda k: (k['namespace'], k['symbol'], k['test'])
    oracle_skips = collections.Counter(map(skip, reference['skips']))
    suss_skips = collections.Counter(map(skip, observed['skips']))
    mismatches = (oracle_skips - suss_skips) + (suss_skips - oracle_skips)
    skip_mismatches = sorted(([*key, count] for key, count in mismatches.items()), key=str)
    counts['skipped'] = sum((oracle_skips & suss_skips).values())
    counts['fail'] += sum(mismatches.values())
    counts['namespaces-failed'] = len(namespace_failures)
    return counts, {'schema': 1, 'suite': COMMIT, 'namespaces': dict(sorted(namespace_failures.items())),
                    'tests': dict(sorted(test_failures.items())), 'assertions': dict(sorted(failures.items())),
                    'skip-mismatches': skip_mismatches}


def reference_text(reference):
    """One assertion per line: compact, with per-assertion diffs."""
    head = {key: value for key, value in reference.items() if key != 'assertions'}
    lines = [json.dumps(entry, ensure_ascii=True, separators=(',', ':')) for entry in reference['assertions']]
    body = json.dumps(head, indent=1, ensure_ascii=True)[:-2]
    return body + ',\n "assertions": [\n' + ',\n'.join(lines) + '\n ]\n}\n'


def counts_line(counts):
    return ' '.join(f'{key}={counts[key]}' for key in sorted(counts))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('command', nargs='?', default='check',
                        choices=('check', 'lock', 'oracle', 'compare', 'overlap', 'suss-sources'))
    parser.add_argument('--suite', default='suite', choices=sorted(SUITES))
    parser.add_argument('--write', action='store_true', help='record instead of comparing')
    parser.add_argument('--git', type=Path, help='also verify against an upstream checkout')
    parser.add_argument('--observations', type=Path, help='Suss observations to compare')
    parser.add_argument('--output', type=Path, help='empty directory for suss-sources')
    args = parser.parse_args()
    if args.command == 'lock' and args.write:
        LOCK.write_text(json.dumps(lock_data(manifest(UPSTREAM)), indent=2) + '\n')
    lock = json.loads(LOCK.read_text())
    verify(lock)
    if args.git and git_manifest(args.git) != lock['files']:
        raise SystemExit(f'vendored files differ from {REPOSITORY}@{COMMIT}')
    suite = SUITES[args.suite]
    if args.command == 'suss-sources':
        generate_suss_sources(args.suite, args.output)
        return
    if args.command == 'overlap':
        text = json.dumps(legacy_overlap(), indent=1, ensure_ascii=True) + '\n'
        if args.write:
            LEGACY_OVERLAP.write_text(text)
        elif LEGACY_OVERLAP.read_text() != text:
            raise SystemExit(f'{LEGACY_OVERLAP.relative_to(ROOT)} is stale; regenerate it with overlap --write')
        print('legacy overlap:', json.loads(text)['summary'])
        return
    if args.command in ('check', 'lock'):
        print(f"clojure-test-suite lock: {len(lock['files'])} files at {COMMIT[:12]}; "
              f"{verify_interop()} classified js/ references")
        return
    if args.command == 'oracle':
        reference = normalize(*run_oracle(args.suite), suite=args.suite)
        for key, count in sorted(summarize(reference).items(), key=str):
            print('assertions', *key, count)
        print('skips', len(reference['skips']), 'tests', len(reference['tests']))
        text = reference_text(reference)
        if json.loads(text) != reference:
            raise AssertionError('reference serialization is not lossless')
        if args.write:
            suite['reference'].write_text(text)
        elif suite['reference'].read_text() != text:
            raise SystemExit(f"fresh oracle observations differ from {suite['reference'].relative_to(ROOT)}; review the diff")
        return
    reference = json.loads(suite['reference'].read_text(), object_pairs_hook=unique)
    observed = json.loads(args.observations.read_text(), object_pairs_hook=unique)
    counts, failures = compare(reference, observed)
    print(f'clojure-test-suite {args.suite}:', counts_line(counts))
    text = json.dumps(failures, indent=1, ensure_ascii=True) + '\n'
    if args.write:
        suite['baseline'].write_text(text)
        return
    baseline = json.loads(suite['baseline'].read_text(), object_pairs_hook=unique)
    if baseline != failures:
        for key in ('namespaces', 'tests', 'assertions'):
            for identity in sorted(set(baseline[key]) | set(failures[key])):
                if baseline[key].get(identity) != failures[key].get(identity):
                    print(f'{key} {identity}: expected {baseline[key].get(identity)!r}, '
                          f'observed {failures[key].get(identity)!r}')
        if baseline['skip-mismatches'] != failures['skip-mismatches']:
            print('skip mismatches changed:', failures['skip-mismatches'])
        raise SystemExit('Suss results differ from the reviewed known-failure baseline '
                         '(new or changed failures, or unexpected passes)')


if __name__ == '__main__':
    main()
