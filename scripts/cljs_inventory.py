#!/usr/bin/env python3
"""Inventory declarations without executing upstream code or interpreting JS.

This is a source-form scanner, not the Suss reader or a portability classifier.
It preserves both .cljc branches. Metadata and quotation remain distinct nodes.
"""
import argparse
import hashlib
import json
import subprocess
from dataclasses import dataclass
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PIN = 'c4295f303100bbf5afac449242d30bca1126f1a1'
SOURCES = ('src/main/cljs/cljs/core.cljs', 'src/main/clojure/cljs/core.cljc')
OUTPUT = ROOT / 'docs/compatibility/cljs-core.edn'
DECLARATIONS = {'def', 'defonce', 'defn', 'defn-', 'defmacro', 'defmacro-',
                'defprotocol', 'deftype', 'defrecord', 'defmulti', 'goog-define'}


@dataclass
class Form:
    kind: str
    start: int
    end: int
    text: str = ''
    children: tuple = ()


class Scanner:
    def __init__(self, source):
        self.source, self.pos = source, 0

    def whitespace(self):
        while self.pos < len(self.source):
            c = self.source[self.pos]
            if c.isspace() or c == ',':
                self.pos += 1
            elif c == ';':
                end = self.source.find('\n', self.pos)
                self.pos = len(self.source) if end < 0 else end + 1
            else:
                break

    def read(self):
        self.whitespace()
        start = self.pos
        if start >= len(self.source):
            raise ValueError('unexpected end of source')
        source = self.source
        c = source[start]
        for prefix, kind, count in (('#?@', 'conditional-splice', 1),
                                    ('#?', 'conditional', 1), ('#_', 'discard', 1),
                                    ("#'", 'var', 1), ('~@', 'splice', 1),
                                    ('^', 'metadata', 2), ("'", 'quote', 1),
                                    ('`', 'syntax-quote', 1), ('~', 'unquote', 1),
                                    ('@', 'deref', 1)):
            if source.startswith(prefix, start):
                self.pos += len(prefix)
                children = tuple(self.read() for _ in range(count))
                return Form(kind, start, self.pos, children=children)
        if c == '"' or source.startswith('#"', start):
            self.pos += 1 if c == '"' else 2
            while self.pos < len(source):
                ch = source[self.pos]
                self.pos += 1
                if ch == '\\':
                    self.pos += 1
                elif ch == '"':
                    return Form('string', start, self.pos, source[start:self.pos])
            raise ValueError(f'unterminated string at {start}')
        if c == '\\':
            self.pos += 2  # A character may itself be a delimiter.
            while self.pos < len(source) and not self.delimiter(source[self.pos]):
                self.pos += 1
            return Form('char', start, self.pos, source[start:self.pos])
        dispatch = source.startswith(('#{', '#('), start)
        if c in '([{' or dispatch:
            if dispatch:
                self.pos += 1
                c = source[self.pos]
            close = {'(': ')', '[': ']', '{': '}'}[c]
            kind = {'(': 'list', '[': 'vector', '{': 'map'}[c]
            if dispatch:
                kind = 'set' if c == '{' else 'anonymous-fn'
            self.pos += 1
            children = []
            while True:
                self.whitespace()
                if self.pos >= len(source):
                    raise ValueError(f'unclosed {c} at {start}')
                if source[self.pos] == close:
                    self.pos += 1
                    return Form(kind, start, self.pos, children=tuple(children))
                children.append(self.read())
        if c in ')]}':
            raise ValueError(f'unexpected {c} at {start}')
        while self.pos < len(source) and not self.delimiter(source[self.pos]):
            self.pos += 1
        if self.pos == start:
            raise ValueError(f'unrecognized token at {start}')
        token = source[start:self.pos]
        # Tagged literals and namespaced maps own their following form.
        if token.startswith('#') and not token.startswith('##'):
            child = self.read()
            return Form('tagged', start, self.pos, token, (child,))
        return Form('atom', start, self.pos, token)

    @staticmethod
    def delimiter(c):
        return c.isspace() or c in ',;()[]{}"`~@^'

    def all(self):
        result = []
        self.whitespace()
        while self.pos < len(self.source):
            result.append(self.read())
            self.whitespace()
        return result


def bare(form):
    while form.kind == 'metadata':
        form = form.children[1]
    return form


def declarations(form, context=()):
    if form.kind in ('conditional', 'conditional-splice'):
        branches = form.children[0].children
        if len(branches) % 2:
            raise ValueError('reader conditional needs feature/form pairs')
        for feature, branch in zip(branches[::2], branches[1::2]):
            items = branch.children if form.kind == 'conditional-splice' else (branch,)
            for item in items:
                yield from declarations(item, context + (feature.text,))
    elif form.kind == 'list' and form.children:
        head = bare(form.children[0]).text.split('/')[-1]
        if head in DECLARATIONS:
            name = bare(form.children[1])
            if name.kind != 'atom':
                raise ValueError(f'non-symbol declaration at {form.start}')
            yield head, name.text, form, context
        # Only module-level executable containers; never descend into a function,
        # macro body, quote or data literal and mistake its templates for defs.
        elif head in ('do', 'let', 'let*', 'binding'):
            offset = 1 if head == 'do' else 2
            for child in form.children[offset:]:
                yield from declarations(child, context + (head,))


def generate():
    pin = subprocess.check_output(['git', '-C', str(ROOT / 'clojurescript'),
                                   'rev-parse', 'HEAD'], text=True).strip()
    if pin != PIN:
        raise ValueError(f'upstream revision {pin} differs from reviewed pin {PIN}')
    dirty = subprocess.check_output(['git', '-C', str(ROOT / 'clojurescript'),
                                     'status', '--porcelain', '--', *SOURCES], text=True)
    if dirty:
        raise ValueError('upstream source has uncommitted changes')
    records = []
    for path in SOURCES:
        source = (ROOT / 'clojurescript' / path).read_text()
        phase = 'runtime' if path.endswith('.cljs') else 'macro'
        for top in Scanner(source).all():
            for kind, name, form, context in declarations(top):
                line = source.count('\n', 0, form.start) + 1
                end_line = source.count('\n', 0, form.end) + 1
                digest = hashlib.sha256(source[form.start:form.end].encode()).hexdigest()
                records.append({'id': f'{phase}:{name}:{line}', 'name': name,
                                'phase': phase, 'kind': kind, 'source': 'clojurescript/' + path,
                                'line': line, 'end-line': end_line, 'sha256': digest,
                                'context': list(context)})
    q = lambda x: json.dumps(x, ensure_ascii=False)
    lines = [';; Generated by scripts/cljs_inventory.py; reviews live in reviews.edn.',
             '{:schema 1', f' :upstream-commit {q(PIN)}', ' :definitions [']
    for r in records:
        lines.append('  {:id ' + q(r['id']) + ' :name ' + q(r['name']) +
                     ' :phase :' + r['phase'] + ' :kind :' + r['kind'])
        lines.append('   :source ' + q(r['source']) + ' :line ' + str(r['line']) +
                     ' :end-line ' + str(r['end-line']) + ' :sha256 ' + q(r['sha256']))
        lines.append('   :reader-context [' + ' '.join(q(x) for x in r['context']) +
                     '] :classification :unassessed :status :unassessed}')
    lines.append(' ]}\n')
    return '\n'.join(lines), len(records)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    content, count = generate()
    if args.check:
        if not OUTPUT.exists() or OUTPUT.read_text() != content:
            raise SystemExit('inventory is stale; run python3 scripts/cljs_inventory.py')
    else:
        OUTPUT.write_text(content)
    print(f'{count} upstream declarations; inventory ' + ('verified' if args.check else 'written'))


if __name__ == '__main__':
    main()
