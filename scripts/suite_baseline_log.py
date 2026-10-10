#!/usr/bin/env python3
"""Audit a complete comparator failure-map delta; never synthesize raw observations."""
import ast
import collections
import copy
import json
import re


def project(original, reference, log):
    summary = re.findall(r'clojure-test-suite suite: ([^\r\n]+)', log)
    if len(summary) != 1 or 'Suss results differ from the reviewed known-failure baseline' not in log:
        raise ValueError('one completed failing comparator report required')
    counts = dict((k, int(v)) for k, v in re.findall(r'([\w-]+)=(\d+)', summary[0]))
    if set(counts) != {'pass', 'guest-judged', 'fail', 'not-executed', 'skipped', 'namespaces-failed'}:
        raise ValueError('complete summary required')
    if counts['guest-judged'] or counts['skipped'] or original['tests']:
        raise ValueError('this logged projection requires host-decided assertions, no test failures/skips')
    if 'skip mismatches changed:' in log:
        raise ValueError('skip-map change requires separate raw evidence')
    result = copy.deepcopy(original)
    changes = []
    seen = set()
    for category, identity, expected, observed in re.findall(
            r'(namespaces|tests|assertions) ([^\r\n]+?): expected (.*?), observed ([^\r\n]+)', log):
        old, new = ast.literal_eval(expected), ast.literal_eval(observed)
        if (category, identity) in seen or result[category].get(identity) != old or old == new:
            raise ValueError('duplicate, stale, or nonchanging failure delta')
        if category == 'tests':
            raise ValueError('test-level change requires separate raw evidence')
        seen.add((category, identity))
        if new is None:
            result[category].pop(identity)
        else:
            result[category][identity] = new
        changes.append({'category': category, 'identity': identity, 'before': old, 'after': new})
    if not changes:
        raise ValueError('failure-map changes required')
    known_ids = {a['id'] for a in reference['assertions']}
    if not set(result['assertions']) <= known_ids or not set(result['namespaces']) <= set(reference['namespaces']):
        raise ValueError('unknown identity')
    rows = []
    totals = collections.Counter()
    for a in reference['assertions']:
        ns = a['test'].split('/')[0]
        if ns in result['namespaces']:
            status, reason = 'not-executed', 'namespace:' + result['namespaces'][ns]['stage']
        elif a['id'] in result['assertions']:
            reason = result['assertions'][a['id']]
            status = 'not-executed' if reason.startswith(('namespace:', 'test:', 'missing')) else 'fail'
        else:
            status, reason = 'pass', None
        totals[status] += 1
        rows.append({'id': a['id'], 'classification': status, 'reason': reason})
    totals['fail'] += sum(x[3] for x in result['skip-mismatches'])
    totals['guest-judged'] = 0
    totals['skipped'] = 0
    totals['namespaces-failed'] = len(result['namespaces'])
    if dict(totals) != counts:
        raise ValueError('full identity projection disagrees with logged counters')
    for category in ('namespaces', 'tests', 'assertions'):
        result[category] = dict(sorted(result[category].items()))
    return result, {'counts': counts, 'changes': changes, 'assertions': rows,
                    'evidence_kind': 'logged comparator classification; not raw native observations'}
