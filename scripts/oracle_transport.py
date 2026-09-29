#!/usr/bin/env python3
"""Validate lossless reference observations; this is not a Suss result decoder."""
import argparse
import json
import re
from pathlib import Path

SCHEMA = 2
PIN = 'c4295f303100bbf5afac449242d30bca1126f1a1'
IDS = ('condition-once', 'argument-order', 'binary64-rounding', 'negative-zero',
       'positive-infinity', 'nan', 'utf16-surrogate', 'utf16-pair', 'nested-values',
       'reduce-empty', 'variadic-arity', 'exception-effect', 'throw-string', 'throw-nil', 'throw-map', 'throw-finally')
EXCEPTIONS = ('exception-effect', 'throw-string', 'throw-nil', 'throw-map', 'throw-finally')


def fields(value, expected):
    if not isinstance(value, dict) or set(value) != set(expected.split()):
        raise ValueError('missing/unknown transport fields')


def value(node, depth=0):
    if depth > 128 or not isinstance(node, dict):
        raise ValueError('invalid/oversized tagged value')
    tag = node.get('tag')
    if tag == 'nil':
        fields(node, 'tag')
    elif tag == 'bool':
        fields(node, 'tag value')
        if type(node['value']) is not bool:
            raise ValueError('invalid Boolean')
    elif tag == 'f64':
        fields(node, 'tag bits')
        if not isinstance(node['bits'], str) or not re.fullmatch('[0-9a-f]{16}', node['bits']):
            raise ValueError('invalid binary64 bits')
    elif tag == 'string':
        fields(node, 'tag units')
        if not isinstance(node['units'], list) or any(type(n) is not int or not 0 <= n <= 65535 for n in node['units']):
            raise ValueError('invalid UTF-16 units')
    elif tag == 'exception-info':
        fields(node, 'tag data message cause')
        for key in ('data', 'message', 'cause'):
            value(node[key], depth + 1)
        if node['message']['tag'] != 'string':
            raise ValueError('invalid ExceptionInfo message')
    elif tag in ('keyword', 'symbol'):
        fields(node, 'tag namespace name')
        value(node['namespace'], depth + 1)
        value(node['name'], depth + 1)
        if node['namespace']['tag'] not in ('nil', 'string') or node['name']['tag'] != 'string':
            raise ValueError('invalid identifier')
    elif tag in ('vector', 'seq', 'set', 'map'):
        key = 'entries' if tag == 'map' else 'items'
        fields(node, 'tag ' + key)
        if not isinstance(node[key], list):
            raise ValueError('invalid collection')
        for item in node[key]:
            if tag == 'map':
                if not isinstance(item, list) or len(item) != 2:
                    raise ValueError('invalid map entry')
                for element in item:
                    value(element, depth + 1)
            else:
                value(item, depth + 1)
    else:
        raise ValueError(f'unknown transport tag {tag}')


def validate(document):
    fields(document, 'schema upstream cases')
    if type(document['schema']) is not int or document['schema'] != SCHEMA or document['upstream'] != PIN:
        raise ValueError('wrong schema/upstream pin')
    cases = document['cases']
    if not isinstance(cases, list) or [c.get('id') if isinstance(c, dict) else None for c in cases] != list(IDS):
        raise ValueError('missing/duplicate/changed oracle case IDs')
    for case in cases:
        expected_status = 'exception' if case['id'] in EXCEPTIONS else 'value'
        if case.get('status') != expected_status:
            raise ValueError('unexpected observation status')
        if case.get('status') == 'value':
            fields(case, 'id status value effects')
            value(case['value'])
        elif case.get('status') == 'exception':
            fields(case, 'id status thrown data message effects')
            value(case['thrown'])
            value(case['data'])
            value(case['message'])
            if case['message']['tag'] not in ('nil', 'string'):
                raise ValueError('invalid exception message')
        else:
            raise ValueError('unknown observation status')
        if not isinstance(case['effects'], list):
            raise ValueError('invalid effects')
        for effect in case['effects']:
            value(effect)
    by_id = {c['id']: c for c in cases}
    for identity, bits in {'condition-once': '3ff0000000000000', 'argument-order': '4008000000000000',
                           'binary64-rounding': '4340000000000000', 'negative-zero': '8000000000000000',
                           'positive-infinity': '7ff0000000000000', 'nan': '7ff8000000000000'}.items():
        if by_id[identity].get('value') != {'tag': 'f64', 'bits': bits}:
            if identity == 'nan' and by_id[identity].get('value') == {'tag': 'f64', 'bits': 'fff8000000000000'}:
                continue
            raise ValueError(f'wrong reference boundary {identity}')
    for identity, units in {'utf16-surrogate': [55296], 'utf16-pair': [55357, 56832]}.items():
        if by_id[identity].get('value') != {'tag': 'string', 'units': units}:
            raise ValueError(f'lossy reference string {identity}')
    expected_effects = {'condition-once': ['condition'], 'argument-order': ['left', 'right'],
                        'exception-effect': ['before-throw'], 'throw-string': ['before-string'],
                        'throw-nil': ['before-nil'], 'throw-map': ['before-map'], 'throw-finally': ['body', 'cleanup']}
    for identity, case in by_id.items():
        labels = []
        for effect in case['effects']:
            if effect['tag'] != 'keyword' or effect['namespace'] != {'tag': 'nil'}:
                raise ValueError('invalid effect label')
            labels.append(''.join(chr(n) for n in effect['name']['units']))
        if labels != expected_effects.get(identity, []):
            raise ValueError('wrong ordered effects')
    if by_id['exception-effect']['status'] != 'exception':
        raise ValueError('missing expected exception')
    return len(cases)


def unique(pairs):
    result = {}
    for key, item in pairs:
        if key in result:
            raise ValueError('duplicate JSON key')
        result[key] = item
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('input', type=Path)
    args = parser.parse_args()
    count = validate(json.loads(args.input.read_text(), object_pairs_hook=unique))
    print(f'{count} reference observations verified; no differential Suss pass is implied')


if __name__ == '__main__':
    main()
