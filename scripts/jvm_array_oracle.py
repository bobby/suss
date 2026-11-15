#!/usr/bin/env python3
"""Record JVM Clojure 1.12's array behavior for Suss's array tests (#425).

Suss's arrays are Clojure's: fixed length, nil-filled, with bounds errors.
Each case is Clojure source; this tool evaluates it on JVM Clojure and
records the printed result as the case's expectation, which
crates/suss-cli/tests/portable_arrays.rs checks Suss against.

Suss keeps ClojureScript's `array` and its `make-array` signatures,
`(make-array n)` and `(make-array type n & dims)`, whose type is ignored;
the JVM prelude defines them over Object arrays. An error is recorded as
[:threw message] where the JVM's message is portable, as the bounds and
negative size messages are, and as :threw otherwise; a case writes
(catch Exception e ...), which the Suss test reads as (catch :default e ...).

    python3 scripts/jvm_array_oracle.py           # rewrite the corpus
    python3 scripts/jvm_array_oracle.py --check   # the corpus is current

Regenerating needs `clojure` on the path; checking does not.
"""
import json
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CORPUS = ROOT / 'tests/oracle/jvm-array-cases.json'
CLOJURE = '1.12.6'

PRELUDE = '''(ns jvm-array-oracle (:refer-clojure :exclude [make-array]))
(defn array [& xs] (object-array (or xs [])))
(defn make-array
  ([n] (clojure.core/make-array Object n))
  ([t n] (clojure.core/make-array Object n))
  ([t n & dims] (apply clojure.core/make-array Object n dims)))'''

THREW = '(catch Exception e [:threw (ex-message e)])'
ANY = '(catch Exception e :threw)'

CASES = [
    # Construction, length and reads.
    ('empty-array-length', '(alength (array))'),
    ('array-length', '(alength (array 1 nil false 4))'),
    ('array-number', '(aget (array 7 8) 1)'),
    ('array-nil', '(nil? (aget (array 7 nil) 1))'),
    ('array-false', '(false? (aget (array false) 0))'),
    ('array-identity', '(let [a (array 1)] (identical? a a))'),
    ('arrays-distinct', '(identical? (array 1) (array 1))'),
    ('array-fractional-index', '(aget (array 1 2) 1.5)'),
    # Bounds: reads and writes outside the array throw.
    ('aget-past-end', f'(try (aget (array 1) 5) {THREW})'),
    ('aget-negative', f'(try (aget (array 1) -1) {THREW})'),
    ('aget-empty', f'(try (aget (array) 0) {THREW})'),
    ('aset-past-end', f'(try (aset (array 1) 3 7) {THREW})'),
    ('aset-past-end-keeps-length', f'(let [a (array 1)] (try (aset a 3 7) {ANY}) (alength a))'),
    ('aget-nil', f'(try (let [a (identity nil)] (aget a 0)) {ANY})'),
    ('aset-nil', f'(try (let [a (identity nil)] (aset a 0 1)) {ANY})'),
    ('alength-nil', f'(try (let [a (identity nil)] (alength a)) {ANY})'),
    ('alength-number', f'(try (let [a (identity 5)] (alength a)) {ANY})'),
    # Writes.
    ('array-mutation-return', '(let [a (array 1)] (aset a 0 9))'),
    ('array-mutation-alias', '(let [a (array 1) b a] (aset a 0 9) (aget b 0))'),
    ('array-holds-any-value', '(let [a (array 1)] (aset a 0 "x") (aset a 0 :k) (aget a 0))'),
    # aclone.
    ('clone-length', '(alength (aclone (array 1 2 3)))'),
    ('clone-distinct', '(let [a (array 1)] (identical? a (aclone a)))'),
    ('clone-isolated', '(let [a (array 1) b (aclone a)] (aset b 0 9) (aget a 0))'),
    ('clone-shallow', '(let [inner (array 7) a (array inner) b (aclone a)] (identical? (aget a 0) (aget b 0)))'),
    # make-array: nil-filled, sizes truncated to int, negative sizes throw.
    ('make-array-length', '(alength (make-array 3))'),
    ('make-array-nil-filled', '(vec (make-array 3))'),
    ('make-array-dynamic-nil-filled', '(let [n 2] (vec (make-array n)))'),
    ('make-array-type-ignored', '(alength (make-array nil 2))'),
    ('make-array-first-class', '(let [f make-array] (vec (f 2)))'),
    ('make-array-first-class-type', '(let [f make-array] (alength (f nil 4)))'),
    ('make-array-zero', '(alength (make-array 0))'),
    ('make-array-fractional', '(alength (make-array 1.5))'),
    ('make-array-dynamic-fractional', '(let [n 2.5] (alength (make-array n)))'),
    ('make-array-negative', f'(try (make-array -2) {THREW})'),
    ('make-array-dynamic-negative', f'(try (let [n -1] (make-array n)) {THREW})'),
    ('make-array-dimensions', '(let [a (make-array nil 2 3)] [(alength a) (alength (aget a 0)) (vec (aget a 1))])'),
    ('make-array-dimensions-distinct', '(let [a (make-array nil 2 3)] (identical? (aget a 0) (aget a 1)))'),
    ('make-array-three-dimensions', '(let [a (make-array nil 2 1 3)] [(alength (aget a 1)) (alength (aget a 1 0)) (aget a 1 0 2)])'),
    ('make-array-dimension-negative', f'(try (make-array nil 2 -1) {THREW})'),
    ('make-array-outer-negative', f'(try (make-array nil -2 3) {THREW})'),
    ('make-array-empty-outer', '(alength (make-array nil 0 3))'),
    ('make-array-empty-outer-negative-inner', f'(try (make-array nil 0 -1) {THREW})'),
    ('make-array-first-negative-dimension', f'(try (make-array nil -3 -1) {THREW})'),
    # Nested reads and writes take several indices.
    ('nested-get', '(aget (array (array 1 2)) 0 1)'),
    ('nested-set', '(let [a (array (array 1 2))] [(aset a 0 1 9) (aget a 0 1)])'),
    ('nested-get-past-end', f'(try (aget (array (array 1 2)) 0 2) {ANY})'),
    # First-class functions.
    ('array-first-class', '(let [f array] (vec (f 1 2 3)))'),
    ('aget-first-class', '(let [f aget] (f (array 5 6) 1))'),
    ('aset-first-class', '(let [f aset a (array 1)] (f a 0 8) (aget a 0))'),
    ('alength-first-class', '(let [f alength] (f (array 1 2)))'),
    ('aclone-first-class', '(let [f aclone] (vec (f (array 1 2))))'),
    # Evaluation order: each operand once, left to right.
    ('array-once-order', '(let [n (atom 0) tick (fn [] (swap! n inc))] (let [a (array (tick) (tick))] [(vec a) @n]))'),
    ('aget-once-order', '(let [n (atom []) tick (fn [x v] (swap! n conj x) v)] [(aget (tick :a (array 7 8)) (tick :i 1)) @n])'),
    ('aset-once-order', '(let [n (atom []) tick (fn [x v] (swap! n conj x) v) a (array 0)] [(aset (tick :a a) (tick :i 0) (tick :v 5)) @n])'),
    ('make-array-dimensions-order', '(let [n (atom []) tick (fn [x v] (swap! n conj x) v)] (make-array (tick :t nil) (tick :a 2) (tick :b 2)) @n)'),
    ('array-function-value-survives', '(let [f (fn [x] (+ x 1)) a (array f)] ((aget a 0) 7))'),
    # Conversions.
    ('to-array-vector', '(vec (to-array [1 2 3]))'),
    ('to-array-nil', '(alength (to-array nil))'),
    ('to-array-seq', '(vec (to-array (map inc (range 3))))'),
    ('to-array-fresh', '(let [v [1 2] a (to-array v)] (aset a 0 9) [v (vec a)])'),
    ('into-array', '(vec (into-array [1 2]))'),
    ('object-array-size', '(vec (object-array 2))'),
    ('object-array-seq', '(vec (object-array [1 2]))'),
    ('array-seq', '(seq (array 1 2))'),
    ('array-empty-seq', '(seq (array))'),
    ('array-count', '(count (array 1 2 3))'),
    ('array-nth', '(nth (array 1 2 3) 2)'),
    ('array-nth-past-end', f'(try (nth (array 1) 4) {ANY})'),
    ('array-nth-not-found', '(nth (array 1) 4 :none)'),
    ('array-get', '[(get (array 1 2) 1) (get (array 1 2) 5) (get (array 1 2) 5 :none)]'),
    ('array-reduce', '(reduce + (array 1 2 3))'),
    ('array-into-vector', '(into [] (array 1 2))'),
    ('array-apply', '(apply + (array 1 2 3))'),
    # vec aliases an array of at most 32 elements, as JVM Clojure's does.
    ('vec-aliases-array', '(let [a (array 1 2) v (vec a)] (aset a 0 9) v)'),
    ('array-equality', '[(= (array 1) (array 1)) (let [a (array 1)] (= a a))]'),
]


def suss_source(source):
    """A case's source as Suss reads it."""
    return source.replace('(catch Exception e ', '(catch :default e ')


def record():
    program = PRELUDE + '\n(doseq [[id form] ' + '[' + ' '.join(
        f'[{json.dumps(i)} (quote {s})]' for i, s in CASES) + ']]\n'
    program += '  (println (str id \"\\t\" (pr-str (try (eval form) (catch Throwable t [:oracle-error (str (class t)) (ex-message t)]))))))\n'
    with tempfile.TemporaryDirectory() as directory:
        path = Path(directory) / 'oracle.clj'
        path.write_text(program)
        output = subprocess.run(
            ['clojure', '-Sdeps', '{:deps {org.clojure/clojure {:mvn/version "%s"}}}' % CLOJURE,
             '-M', str(path)],
            capture_output=True, text=True)
        if output.returncode:
            raise SystemExit(output.stderr + output.stdout)
        output = output.stdout
    observed = {}
    for line in output.splitlines():
        identifier, printed = line.split('\t', 1)
        observed[identifier] = printed
    errors = [i for i, printed in observed.items() if printed.startswith('[:oracle-error')]
    if errors:
        raise SystemExit('cases that JVM Clojure cannot evaluate: ' + ', '.join(f'{i} {observed[i]}' for i in errors))
    cases = []
    for identifier, source in CASES:
        cases.append({'id': identifier, 'source': suss_source(source), 'expected': observed[identifier]})
    return {'schema': 1, 'clojure': CLOJURE, 'cases': cases}


def check():
    corpus = json.loads(CORPUS.read_text())
    expected = [(identifier, suss_source(source)) for identifier, source in CASES]
    if [(case['id'], case['source']) for case in corpus['cases']] != expected:
        raise SystemExit('stale corpus; run python3 scripts/jvm_array_oracle.py')
    print(f'{len(expected)} JVM Clojure array cases current')


if __name__ == '__main__':
    if sys.argv[1:] == ['--check']:
        check()
    elif not sys.argv[1:]:
        CORPUS.write_text(json.dumps(record(), indent=2) + '\n')
        print(f'{len(CASES)} JVM Clojure {CLOJURE} array cases recorded')
    else:
        raise SystemExit('usage: jvm_array_oracle.py [--check]')
