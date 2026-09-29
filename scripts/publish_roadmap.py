#!/usr/bin/env python3
"""Publish missing Suss milestones/issues; defaults to an offline preview.

Requires authenticated gh and network access. Uses stable body markers for
idempotency. Existing issues are preserved, not overwritten or closed.
"""
import argparse
import json
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / 'docs/roadmap/issues.json'
LINKS = ROOT / 'docs/roadmap/github.json'
MILESTONES = {
    'M0': 'Contract and feasibility', 'M1': 'Trustworthy evidence',
    'M2': 'Compiler and runtime foundation', 'M3': 'Persistent development environment',
    'M4': 'Portable persistent collections', 'M5': 'Generic WIT interoperability',
    'M6': 'WASI 0.3.1 alpha', 'M7': 'Portable compatibility beta',
    'M8': 'Browser beta', 'M9': 'CSP extension',
}


def api(path, payload=None):
    command = ['gh', 'api', path]
    if payload is None:
        command += ['--paginate', '--slurp']
        pages = json.loads(subprocess.check_output(command, text=True))
        return [item for page in pages for item in page]
    command += ['--method', 'POST', '--input', '-']
    return json.loads(subprocess.check_output(command, input=json.dumps(payload), text=True))


def body(issue, urls):
    deps = ', '.join(f'[{dep}]({urls[dep]})' if dep in urls else dep for dep in issue['dependencies']) or 'None'
    return f'''<!-- suss-roadmap:{issue['id']} -->
{issue['objective']}

Specification: `docs/design/suss-0.3.1.md`, sections {issue['spec_sections']}.
Repository work package: `{issue['id']}` in `docs/roadmap/issues.json`.
Dependencies: {deps}.

## Acceptance criteria

{issue['acceptance']}

## Scope and interfaces

{issue['scope']} Implement the public contracts in the referenced specification;
record any changed API/ABI and affected inventory entries with the implementation.

## Validation

Proposed acceptance command (the suite may need to be created):

```sh
{issue['validation']}
```

Add a regression that fails before the implementation. Validate and execute
artifacts where relevant; encoding success alone is insufficient. Record the
commands, results, remaining gaps and next task in `docs/roadmap/handoff.md`.

Status when published: {issue['status']}. Do not close until all acceptance
criteria have evidence. This issue does not claim that planned APIs exist today.
'''


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--publish', action='store_true')
    args = parser.parse_args()
    data = json.loads(SOURCE.read_text())
    ids = [i['id'] for i in data['issues']]
    assert len(set(ids)) == len(ids), 'duplicate issue IDs'
    assert all(d in ids or d in MILESTONES for i in data['issues'] for d in i['dependencies'])
    if not args.publish:
        print(f"Offline preview: {len(MILESTONES)} milestones, {len(ids)} issues for {data['repository']}")
        return
    base = f"repos/{data['repository']}"
    milestones = api(base + '/milestones?state=all&per_page=100')
    existing = api(base + '/issues?state=all&per_page=100')
    urls, numbers = {}, {}
    for key, title in MILESTONES.items():
        title = f'{key}: {title}'
        milestone = next((m for m in milestones if m['title'] == title), None)
        if milestone is None:
            milestone = api(base + '/milestones', {'title': title, 'description':
                f'Suss resurrection {key}. Close only when all linked acceptance criteria have evidence. See docs/design/suss-0.3.1.md and ROADMAP.md.'})
        urls[key], numbers[key] = milestone['html_url'], milestone['number']
    for issue in data['issues']:
        marker = f"<!-- suss-roadmap:{issue['id']} -->"
        matches = [i for i in existing if marker in (i.get('body') or '')]
        if len(matches) > 1:
            raise SystemExit(f'duplicate remote marker: {marker}')
        if matches:
            remote = matches[0]
        else:
            remote = api(base + '/issues', {'title': f"[{issue['id']}] {issue['title']}",
                'body': body(issue, urls), 'milestone': numbers[issue['milestone']]})
        urls[issue['id']] = remote['html_url']
        # Persist after each issue so interrupted publication still leaves links.
        LINKS.write_text(json.dumps({'repository': data['repository'], 'urls': urls}, indent=2) + '\n')
        print(f"{issue['id']}: {remote['html_url']}", flush=True)


if __name__ == '__main__':
    main()
