#!/usr/bin/env python3
"""Check an exact projection of genuine pinned analyzer records and execution.

This checks upstream evidence only. It does not certify native rich &env support.
Compiler-owned catch names are normalized by the macro using their actual role.
"""
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PIN = 'c4295f303100bbf5afac449242d30bca1126f1a1'
LABELS = ['top', 'literal', 'shadow', 'invoke', 'if', 'loop', 'catch',
          'function', 'nested', 'anonymous-hint', 'protocol-parameter',
          'object-parameter', 'protocol-field', 'nested-receiver',
          'protocol-restored', 'object-field']


def unique(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError(f'duplicate key: {key}')
        result[key] = value
    return result


def parse(text):
    def reject(value):
        raise ValueError(f'invalid JSON constant: {value}')
    return json.loads(text, object_pairs_hook=unique, parse_constant=reject)


def require(condition, message):
    if not condition:
        raise ValueError(message)


def optional_string(value):
    return value is None or type(value) is str


def optional_integer(value):
    return value is None or (type(value) is int and value >= 0)


def tag(value):
    require(optional_string(value) or
            (type(value) is list and all(type(s) is str for s in value)
             and value == sorted(set(value))), 'invalid tag fact')


def ast(value):
    if value is None:
        return
    require(type(value) is list and len(value) == 5, 'invalid initializer fact')
    require(value[0] in ['const', 'js', 'invoke', 'if', 'local'], 'unknown initializer op')
    tag(value[1])
    require(optional_string(value[2]), 'invalid source form')
    require(type(value[3]) is list and all(type(s) is str for s in value[3]),
            'invalid AST children')
    require(value[4] in [None, 'statement', 'expr', 'return'], 'invalid AST context')


def binding(value, depth=0):
    if value is None:
        return
    require(depth < 64 and type(value) is list and len(value) == 16, 'invalid binding fact')
    require(value[0] in [None, 'binding'], 'unknown binding op')
    require(optional_string(value[1]) and value[2] in ['let', 'loop', 'arg', 'fn', 'catch', 'field'],
            'invalid binding name/role')
    require(optional_integer(value[3]), 'invalid logical arg ordinal')
    for index in [4, 6, 7, 8, 9, 10]:
        require(value[index] is None or type(value[index]) is bool, 'invalid optional binding flag')
    tag(value[5])
    for index in [11, 12]:
        require(value[index] is None or (type(value[index]) is int and value[index] > 0),
                'invalid declaration location')
    require(value[13] in [None, 'statement', 'expr', 'return'], 'invalid declaration context')
    ast(value[14])
    binding(value[15], depth + 1)


def observations(calls, results):
    require(type(calls) is list and len(calls) == 16, 'missing analyzer observations')
    require([call[0] for call in calls if type(call) is list and len(call) == 5] == LABELS,
            'missing, duplicate or reordered calls')
    for call in calls:
        require(call[1] in ['statement', 'expr', 'return'] and
                call[2] == 'suss-oracle.rich-environment-facts', 'invalid context/namespace')
        require(type(call[3]) is list and type(call[4]) is list, 'invalid locals/scopes')
        names = []
        for local in call[3]:
            require(type(local) is list and len(local) == 2 and type(local[0]) is str,
                    'invalid local entry')
            names.append(local[0])
            binding(local[1])
        require(names == sorted(set(names)), 'invalid ordered local name set')
        for scope in call[4]:
            require(type(scope) is list and len(scope) == 5 and type(scope[0]) is str
                    and scope[1] == 'fn' and type(scope[2]) is bool
                    and type(scope[3]) is list and all(type(s) is str for s in scope[3]),
                    'invalid function scope')
            binding(scope[4])
    require(type(results) is list and len(results) == 13 and
            all(type(value) is int and value >= 0 for value in results), 'missing executed results')


expected = parse((ROOT / 'tests/oracle/rich-environment-observations.json').read_text())
require(type(expected) is dict and set(expected) == {'schema', 'upstream', 'calls', 'results'}
        and type(expected['schema']) is int and expected['schema'] == 1
        and expected['upstream'] == PIN, 'invalid pinned corpus')
actual_calls = [parse(line) for line in
                (ROOT / 'tests/oracle/out/rich-environment-calls.jsonl').read_text().splitlines()]
actual_results = parse((ROOT / 'tests/oracle/out/rich-environment-results.json').read_text())
observations(expected['calls'], expected['results'])
observations(actual_calls, actual_results)
require(actual_calls == expected['calls'] and actual_results == expected['results'],
        'changed pinned rich environment observations')
print('16 pinned rich environment observations and 13 executed results match exactly')
