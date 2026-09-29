#!/usr/bin/env python3
"""Compare independent Suss observations to the pinned reference, tracking exact failures."""
import argparse
import json
from pathlib import Path
from oracle_transport import fields, unique, validate, value, IDS, PIN, SCHEMA

STAGES = ('compile', 'artifact', 'validation', 'instantiate', 'trap', 'execution', 'decode', 'value')


def observation(actual):
    status = actual.get('status') if isinstance(actual, dict) else None
    if status == 'value':
        fields(actual, 'id status value effects')
        value(actual['value'])
    elif status == 'exception':
        fields(actual, 'id status thrown data message effects')
        value(actual['thrown'])
        value(actual['data'])
        value(actual['message'])
        if actual['message']['tag'] not in ('nil', 'string'):
            raise ValueError('invalid exception message')
    else:
        raise ValueError('unknown Suss observation status')
    if not isinstance(actual['effects'], list):
        raise ValueError('invalid effects')
    for effect in actual['effects']:
        value(effect)


def unordered(left, right, equal):
    if len(left) != len(right):
        return False
    used = set()
    for item in left:
        for index, candidate in enumerate(right):
            if index not in used and equal(item, candidate):
                used.add(index)
                break
        else:
            return False
    return True


def matches(left, right):
    if left['tag'] != right['tag']:
        return False
    tag = left['tag']
    if tag == 'exception-info':
        return all(matches(left[key], right[key]) for key in ('data', 'message', 'cause'))
    if tag == 'map':
        return unordered(left['entries'], right['entries'],
                         lambda a, b: matches(a[0], b[0]) and matches(a[1], b[1]))
    if tag == 'set':
        return unordered(left['items'], right['items'], matches)
    if tag in ('seq', 'vector'):
        return len(left['items']) == len(right['items']) and all(matches(a, b) for a, b in zip(left['items'], right['items']))
    return left == right


def compare(reference, observations):
    validate(reference)
    fields(observations, 'schema upstream cases')
    if type(observations['schema']) is not int or observations['schema'] != SCHEMA or observations['upstream'] != PIN:
        raise ValueError('wrong observation schema/pin')
    cases = observations['cases']
    if not isinstance(cases, list) or [c.get('id') if isinstance(c, dict) else None for c in cases] != list(IDS):
        raise ValueError('missing/duplicate/changed observation IDs')
    failures = {}
    for expected, actual in zip(reference['cases'], cases):
        status = actual.get('status')
        if status == 'failure':
            fields(actual, 'id status stage diagnostic')
            if actual['stage'] not in STAGES[:-1] or not isinstance(actual['diagnostic'], str) or not actual['diagnostic'].strip():
                raise ValueError('invalid failure stage/diagnostic')
            failures[actual['id']] = {'stage': actual['stage'], 'diagnostic': actual['diagnostic']}
            continue
        observation(actual)
        same = status == expected['status'] and actual['effects'] == expected['effects']
        if same:
            keys = ('value',) if status == 'value' else ('thrown', 'data', 'message')
            same = all(matches(expected[key], actual[key]) for key in keys)
        if not same:
            failures[actual['id']] = {'stage': 'value', 'expected': expected, 'actual': actual}
    return failures


def baseline(source):
    result = json.loads(source, object_pairs_hook=unique)
    if not isinstance(result, dict):
        raise ValueError('invalid known failures')
    for identity, failure in result.items():
        if identity not in IDS or not isinstance(failure, dict):
            raise ValueError('unknown known-failure ID')
        if failure.get('stage') == 'value':
            fields(failure, 'stage expected actual')
            for key in ('expected', 'actual'):
                observation(failure[key])
                if failure[key]['id'] != identity:
                    raise ValueError('mismatched known-failure ID')
        else:
            fields(failure, 'stage diagnostic')
            if failure['stage'] not in STAGES[:-1] or not isinstance(failure['diagnostic'], str) or not failure['diagnostic'].strip():
                raise ValueError('invalid known failure')
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--reference', required=True, type=Path)
    parser.add_argument('--observations', required=True, type=Path)
    parser.add_argument('--baseline', type=Path)
    args = parser.parse_args()
    reference = json.loads(args.reference.read_text(), object_pairs_hook=unique)
    observations = json.loads(args.observations.read_text(), object_pairs_hook=unique)
    failures = compare(reference, observations)
    print(f'{len(IDS) - len(failures)} differential passing, {len(failures)} failing, 0 skipped')
    if args.baseline:
        expected = baseline(args.baseline.read_text())
        if failures != expected:
            print(json.dumps({'expected_failures': expected, 'actual_failures': failures}, indent=2))
            raise SystemExit('differential baseline changed (including unexpected passes)')
        print('Exact known-failure baseline unchanged; this does not certify compatibility')
    else:
        print(json.dumps(failures, indent=2))
        if failures:
            raise SystemExit(1)


if __name__ == '__main__':
    main()
