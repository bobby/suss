"""Original development-only cross-invocation identity artifact checker."""
import json
from pathlib import Path

EXPECTED = [
    ['first', True, 'before'],
    ['same-revision', True, 'before'],
    ['new-revision', False, 'after'],
    ['same-new-revision', False, 'after'],
]


def compare(rows, actual):
    # JSON framing keeps Boolean metadata distinct from Python's equal integers.
    if json.dumps(rows) != json.dumps(EXPECTED):
        raise ValueError(f'Fresh pinned cross-invocation traces differ: {rows!r}')
    if json.dumps(actual) != json.dumps([EXPECTED[:2], EXPECTED[2:], 17]):
        raise ValueError(f'Executed cross-invocation artifact differs: {actual!r}')


def main():
    root = Path(__file__).resolve().parents[1] / 'tests' / 'oracle' / 'out'
    rows = [json.loads(line) for line in
            (root / 'global-reference-cross-invocation.jsonl').read_text().splitlines()]
    actual = json.loads((root / 'global-reference-cross-invocation-results.json').read_text())
    compare(rows, actual)
    info = [json.loads(line) for line in
            (root / 'global-reference-info-identity.jsonl').read_text().splitlines()]
    if json.dumps(info) != json.dumps([True, False, False, False]):
        raise ValueError(f'Fresh outer info identity differs: {info!r}')
    print('4 fresh pinned cross-invocation identity traces and actual Node projections match')


if __name__ == '__main__':
    main()
