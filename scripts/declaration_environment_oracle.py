#!/usr/bin/env python3
"""Exact primary declaration observations; this does not certify native transport."""
import json
from pathlib import Path
from oracle_transport import fields, unique, PIN

ROOT = Path(__file__).resolve().parents[1]
SOURCE = 'tests/oracle/src/suss_oracle/declaration_runner.cljs'
KEYS = ['name', 'ns', 'tag', 'ret-tag', 'fn-var', 'variadic?', 'max-fixed-arity',
        'method-params', 'arglists', 'arglists-meta', 'private', 'dynamic', 'doc',
        'declared', 'line', 'column', 'file', 'meta']
LABELS = ['initial', 'scalar-initializer', 'after-scalar', 'fixed-body',
          'after-fixed', 'multiple-fixed-body', 'multiple-rest-body',
          'after-multiple', 'after-alias', 'hinted-initializer', 'after-hinted',
          'scalar-redefinition', 'after-redefinition', 'nested-after-first',
          'nested-second-initializer', 'nested-after-second', 'after-nested',
          'after-declare', 'after-declared-definition', 'after-duplicate', 'after-named',
          'after-arglists', 'after-mapped-arglists', 'after-empty-top-fn', 'after-overridden', 'after-declared-meta', 'after-nil-top-fn', 'after-entry-top-fn', 'after-empty-seq-top-fn']
LOCAL_KEYS = ['name', 'local', 'tag', 'fn-var', 'variadic?', 'max-fixed-arity',
              'method-params', 'arglists']
LOCAL_LABELS = ['self-fixed-body', 'self-rest-body']
RESULT = [42, 42, 42, 11, 42, 12, 13, 42, 15, 7, 42, 42, 42, 42,
          42, 1, 42, 42, 42, 16, 42, 2, 42, 18, 19, 42, 42, 21, 22, 42, 24, 42, 42, 42, 42, 42, 42]


def parse(text):
    def reject(value):
        raise ValueError(f'invalid JSON constant: {value}')
    return json.loads(text, object_pairs_hook=unique, parse_constant=reject)


def validate_data(value):
    if value is None or type(value) in (bool, str, int, float):
        return
    if type(value) is not list or len(value) != 2:
        raise ValueError('invalid tagged declaration value')
    kind, items = value
    if kind in ('symbol', 'keyword'):
        if type(items) is not str or not items:
            raise ValueError('invalid identifier value')
    elif kind in ('vector', 'seq', 'set'):
        if type(items) is not list:
            raise ValueError('invalid sequential value')
        for item in items:
            validate_data(item)
    elif kind == 'map':
        if type(items) is not list:
            raise ValueError('invalid metadata map')
        seen = set()
        for pair in items:
            if type(pair) is not list or len(pair) != 2:
                raise ValueError('invalid metadata entry')
            validate_data(pair[0])
            validate_data(pair[1])
            identity = json.dumps(pair[0])
            if identity in seen:
                raise ValueError('duplicate metadata key')
            seen.add(identity)
    else:
        raise ValueError('unknown declaration value kind')


def normalize_file(value, actual):
    # Only observed :file fields may lose the checkout prefix. All other data,
    # including source columns and reader end positions, remains exact.
    expected = str(ROOT / SOURCE) if actual else SOURCE
    if value != expected:
        raise ValueError(f'unexpected declaration source path: {value}')
    return SOURCE


def validate(row, actual=False, local=False):
    keys, labels = (LOCAL_KEYS, LOCAL_LABELS) if local else (KEYS, LABELS)
    if type(row) is not list or len(row) != (3 if local else 4) or row[0] not in labels or row[1] != 'suss-oracle.declaration-runner':
        raise ValueError('invalid declaration observation identity')
    names = []
    for declarations in row[2:]:
        if type(declarations) is not list or not declarations:
            raise ValueError('missing declarations')
        current = []
        for declaration in declarations:
            if type(declaration) is not list or len(declaration) != 3:
                raise ValueError('invalid declaration')
            name, present, properties = declaration
            if type(name) is not str or not name or type(present) is not bool:
                raise ValueError('invalid declaration name or presence')
            current.append(name)
            if type(properties) is not list or len(properties) != len(keys):
                raise ValueError('missing declaration fields')
            for key, field in zip(keys, properties):
                if type(field) is not list or len(field) != 3 or field[0] != key or type(field[1]) is not bool:
                    raise ValueError('invalid declaration field')
                if (not present and field[1]) or (not field[1] and field[2] is not None):
                    raise ValueError('invalid absent declaration field')
                validate_data(field[2])
                if key == 'file' and field[1]:
                    field[2] = normalize_file(field[2], actual)
                if key == 'meta' and field[1]:
                    if type(field[2]) is not list or field[2][0] != 'map':
                        raise ValueError('expected symbol metadata map')
                    for pair in field[2][1]:
                        if pair[0] == ['keyword', ':file']:
                            pair[1] = normalize_file(pair[1], actual)
                if key == 'arglists-meta' and field[1]:
                    if type(field[2]) is not list or field[2][0] not in ('seq', 'vector'):
                        raise ValueError('expected argument-list metadata sequence or vector')
                    for metadata in field[2][1]:
                        if metadata is None:
                            continue
                        if type(metadata) is not list or metadata[0] != 'map':
                            raise ValueError('expected argument-list metadata map')
                        for pair in metadata[1]:
                            if pair[0] == ['keyword', ':file']:
                                pair[1] = normalize_file(pair[1], actual)
        if len(set(current)) != len(current):
            raise ValueError('duplicate declaration name')
        names.append(current)
    if not local and names[0] != names[1]:
        raise ValueError('snapshot and catalog declarations differ')
    return row


def main():
    expected = parse((ROOT / 'tests/oracle/declaration-environment-observations.json').read_text())
    fields(expected, 'schema upstream cases locals result')
    if type(expected['schema']) is not int or expected['schema'] != 1 or expected['upstream'] != PIN:
        raise ValueError('invalid declaration corpus schema or pin')
    if type(expected['cases']) is not list or expected['result'] != RESULT:
        raise ValueError('invalid declaration corpus cases or result')
    cases = [validate(row) for row in expected['cases']]
    actual = [validate(parse(line), actual=True) for line in
              (ROOT / 'tests/oracle/out/declaration-environment-calls.jsonl').read_text().splitlines()]
    if [row[0] for row in cases] != LABELS or actual != cases:
        raise ValueError('fresh pinned declaration observations differ')
    if type(expected['locals']) is not list:
        raise ValueError('invalid local corpus')
    locals_expected = [validate(row, local=True) for row in expected['locals']]
    locals_actual = [validate(parse(line), actual=True, local=True) for line in
                     (ROOT / 'tests/oracle/out/declaration-local-calls.jsonl').read_text().splitlines()]
    if [row[0] for row in locals_expected] != LOCAL_LABELS or locals_actual != locals_expected:
        raise ValueError('fresh pinned self/local observations differ')
    result = parse((ROOT / 'tests/oracle/out/declaration-environment-result.json').read_text())
    if type(result) is not list or any(type(n) is not int for n in result) or result != RESULT:
        raise ValueError('unexpected declaration oracle execution')
    print('29 fresh pinned snapshot/catalog and 2 self/local observations; 37 executed results match exactly; complete native schema remains pending')


if __name__ == '__main__':
    main()
