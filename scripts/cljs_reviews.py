#!/usr/bin/env python3
"""Validate the human review overlay against current, pinned source declarations.

Reads a strict EDN data subset. It does not execute forms or infer portability.
"""
import argparse
from dataclasses import dataclass
from pathlib import Path

from cljs_inventory import PIN, ROOT, Scanner, inventory_records


@dataclass(frozen=True)
class Keyword:
    name: str


def keyword(name):
    return Keyword(':' + name)


def data(form):
    if form.kind == 'string':
        # EDN strings and JSON strings share these escapes; reject anything else.
        import json
        return json.loads(form.text)
    if form.kind == 'atom':
        if form.text.startswith(':'):
            return Keyword(form.text)
        if form.text in ('nil', 'true', 'false'):
            return {'nil': None, 'true': True, 'false': False}[form.text]
        if form.text.isascii() and form.text.isdecimal():
            return int(form.text)
    if form.kind == 'vector':
        return [data(child) for child in form.children]
    if form.kind == 'map':
        if len(form.children) % 2:
            raise ValueError('EDN map requires key/value pairs')
        result = {}
        for key, value in zip(form.children[::2], form.children[1::2]):
            key = data(key)
            if not isinstance(key, (str, Keyword)):
                raise ValueError('review map keys must be strings or keywords')
            if key in result:
                raise ValueError(f'duplicate review map key: {key}')
            result[key] = data(value)
        return result
    raise ValueError(f'unsupported review data form: {form.kind} {form.text}')


def fields(value, names, label):
    expected = {keyword(name) for name in names.split()}
    if not isinstance(value, dict) or value.keys() != expected:
        raise ValueError(f'{label} requires exactly these fields: {names}')
    return {name: value[keyword(name)] for name in names.split()}


def text(value, label):
    if not isinstance(value, str) or not value.strip():
        raise ValueError(f'{label} must be a nonempty string')


def text_list(value, label):
    if not isinstance(value, list):
        raise ValueError(f'{label} must be a vector')
    for item in value:
        text(item, label)
    if len(value) != len(set(value)):
        raise ValueError(f'{label} contains duplicates')


def choice(value, choices, label):
    if not isinstance(value, Keyword) or value not in {keyword(name) for name in choices.split()}:
        raise ValueError(f'{label} must be one of: {choices}')


def validate(source, records):
    forms = Scanner(source).all()
    if len(forms) != 1:
        raise ValueError('review overlay must contain exactly one data form')
    overlay = fields(data(forms[0]), 'schema upstream-commit reviews', 'overlay')
    if type(overlay['schema']) is not int or overlay['schema'] != 1:
        raise ValueError('unsupported review schema')
    if overlay['upstream-commit'] != PIN:
        raise ValueError('review upstream commit differs from inventory pin')
    reviews = overlay['reviews']
    if not isinstance(reviews, dict):
        raise ValueError('reviews must be an ID-keyed map')
    definitions = {record['id']: record for record in records}
    if len(definitions) != len(records):
        raise ValueError('duplicate generated declaration IDs')
    for identity, entry in reviews.items():
        if not isinstance(identity, str) or identity not in definitions:
            raise ValueError(f'unknown review declaration ID: {identity}')
        review = fields(entry, 'source-sha256 visibility arities dependencies classification rationale status adaptation-path alternative tests', identity)
        if review['source-sha256'] != definitions[identity]['sha256']:
            raise ValueError(f'stale source hash for {identity}')
        choice(review['visibility'], 'public private generated', 'visibility')
        choice(review['classification'], 'portable adapted host-specific', 'classification')
        choice(review['status'], 'unimplemented in-progress implemented excluded', 'status')
        text(review['rationale'], 'rationale')
        text_list(review['dependencies'], 'dependencies')
        text_list(review['tests'], 'tests')
        for name in ('adaptation-path', 'alternative'):
            if review[name] is not None:
                text(review[name], name)
        arities = review['arities']
        if definitions[identity].get('kind') in {'defn', 'defn-', 'defmacro', 'defmacro-'} and arities is None:
            raise ValueError('function/macro review requires explicit arities')
        if arities is not None:
            arities = fields(arities, 'fixed variadic-min', 'arities')
            fixed = arities['fixed']
            if not isinstance(fixed, list) or any(type(n) is not int or n < 0 for n in fixed):
                raise ValueError('fixed arities must be nonnegative integers')
            if fixed != sorted(set(fixed)):
                raise ValueError('fixed arities must be sorted and unique')
            minimum = arities['variadic-min']
            if minimum is not None and (type(minimum) is not int or minimum < 0):
                raise ValueError('variadic minimum must be a nonnegative integer or nil')
            if not fixed and minimum is None:
                raise ValueError('callable review requires at least one arity')
        if review['classification'] == keyword('adapted') and review['adaptation-path'] is None:
            raise ValueError('adapted review requires an adaptation path')
        if review['status'] == keyword('implemented') and not review['tests']:
            raise ValueError('implemented review requires test evidence references')
        if review['status'] == keyword('excluded'):
            if review['classification'] != keyword('host-specific') or review['alternative'] is None:
                raise ValueError('exclusion requires host-specific classification and an alternative')
    return len(reviews), len(definitions) - len(reviews)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--reviews', type=Path, default=ROOT / 'docs/compatibility/reviews.edn')
    args = parser.parse_args()
    reviewed, unassessed = validate(args.reviews.read_text(), inventory_records())
    print(f'Review overlay verified: {reviewed} reviewed, {unassessed} unassessed declarations')


if __name__ == '__main__':
    main()
