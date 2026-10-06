import copy
import json
import re
import unittest

import clojure_test_suite as suite

NUMBER_ONE = {'tag': 'f64', 'bits': '3ff0000000000000'}
NUMBER_TWO = {'tag': 'f64', 'bits': '4000000000000000'}
OPAQUE = {'tag': 'opaque', 'type': 'function', 'reason': 'unsupported'}


def run(assertions, **extra):
    document = {'schema': 1, 'suite': suite.COMMIT, 'upstream': suite.CLJS_PIN,
                'namespaces': ['a.b-test'], 'tests': ['a.b-test/t'], 'skips': [],
                'assertions': assertions, 'reporter-errors': [],
                'summary': {'test': 1, 'pass': len(assertions), 'fail': 0, 'error': 0}}
    document.update(extra)
    return document


def oracle_assertion(ordinal=1, operands=(NUMBER_ONE, NUMBER_ONE), test='a.b-test/t'):
    return {'test': test, 'ordinal': ordinal, 'kind': 'predicate', 'verdict': 'pass',
            'form': '(= 1 1)', 'line': 1, 'column': 1, 'contexts': [], 'operands': list(operands)}


def reference(*assertions):
    return suite.normalize(run(list(assertions)), suite='fixture')


def observed(assertions, namespaces=None, test_failures=(), skips=()):
    return {'schema': 1, 'suite': suite.COMMIT,
            'namespaces': namespaces if namespaces is not None else [{'namespace': 'a.b-test', 'status': 'loaded'}],
            'test-failures': list(test_failures), 'skips': list(skips), 'assertions': assertions}


def suss_predicate(ordinal=1, operands=(NUMBER_ONE, NUMBER_ONE), verdict='pass'):
    return {'test': 'a.b-test/t', 'ordinal': ordinal, 'kind': 'predicate', 'verdict': verdict,
            'result': {'tag': 'bool', 'value': verdict == 'pass'}, 'operands': list(operands)}


class Normalize(unittest.TestCase):
    def test_stable_runs_keep_observations(self):
        result = reference(oracle_assertion())
        entry = result['assertions'][0]
        self.assertEqual((entry['id'], entry['deterministic'], entry['operands']),
                         ('a.b-test/t#1', True, [NUMBER_ONE, NUMBER_ONE]))

    def test_unreviewed_nondeterminism_is_rejected(self):
        first = run([oracle_assertion()])
        second = run([oracle_assertion(operands=(NUMBER_ONE, NUMBER_TWO))])
        with self.assertRaisesRegex(ValueError, 'unreviewed nondeterministic'):
            suite.normalize(first, second, suite='fixture')

    def test_reviewed_randomized_assertions_keep_only_verdicts(self):
        test = 'clojure.core-test.shuffle/test-shuffle'
        first = run([oracle_assertion(test=test)])
        second = run([oracle_assertion(test=test, operands=(NUMBER_ONE, NUMBER_TWO))])
        entry = suite.normalize(first, second, suite='fixture')['assertions'][0]
        self.assertFalse(entry['deterministic'])
        self.assertNotIn('operands', entry)

    def test_reporter_errors_and_incomplete_runs_are_rejected(self):
        with self.assertRaisesRegex(ValueError, 'reporter failed'):
            suite.normalize(run([], **{'reporter-errors': [{'error': 'boom'}]}), suite='fixture')
        with self.assertRaisesRegex(ValueError, 'incomplete'):
            suite.normalize(run([oracle_assertion()], summary=None), suite='fixture')

    def test_runs_must_agree_on_assertion_order(self):
        with self.assertRaisesRegex(ValueError, 'assertion order'):
            suite.normalize(run([oracle_assertion()]), run([oracle_assertion(ordinal=2)]), suite='fixture')


class Decide(unittest.TestCase):
    def setUp(self):
        self.expected = reference(oracle_assertion())['assertions'][0]

    def test_matching_operands_pass(self):
        self.assertEqual(suite.decide(self.expected, suss_predicate()), (None, False))

    def test_different_operand_fails_even_when_guest_passes(self):
        self.assertEqual(suite.decide(self.expected, suss_predicate(operands=(NUMBER_ONE, NUMBER_TWO))),
                         ('observation:value', False))

    def test_guest_failure_is_a_failure(self):
        self.assertEqual(suite.decide(self.expected, suss_predicate(verdict='fail')), ('verdict:fail', False))

    def test_opaque_oracle_operand_is_guest_judged(self):
        expected = reference(oracle_assertion(operands=(OPAQUE,)))['assertions'][0]
        self.assertEqual(suite.decide(expected, suss_predicate(operands=(OPAQUE,))), (None, True))

    def test_undecodable_suss_operand_never_passes(self):
        self.assertEqual(suite.decide(self.expected, suss_predicate(operands=(NUMBER_ONE, OPAQUE))),
                         ('observation:undecodable', False))

    def test_oracle_value_kind_uses_suss_predicate_result(self):
        expected = dict(self.expected, kind='value', value={'tag': 'bool', 'value': True})
        del expected['operands']
        self.assertEqual(suite.decide(expected, suss_predicate()), (None, False))

    def test_kind_and_thrown_mismatches_fail(self):
        thrown = dict(self.expected, kind='thrown', threw=True)
        actual = {'kind': 'thrown', 'verdict': 'pass', 'threw': False}
        self.assertEqual(suite.decide(thrown, actual), ('observation:threw', False))
        self.assertEqual(suite.decide(thrown, suss_predicate()), ('kind:predicate', False))


class Compare(unittest.TestCase):
    def setUp(self):
        self.reference = reference(oracle_assertion(1), oracle_assertion(2))

    def test_all_passing(self):
        counts, failures = suite.compare(self.reference, observed([suss_predicate(1), suss_predicate(2)]))
        self.assertEqual((counts['pass'], counts['fail']), (2, 0))
        self.assertEqual(failures['assertions'], {})

    def test_failed_namespace_accounts_for_all_assertions(self):
        failed = [{'namespace': 'a.b-test', 'status': 'failure', 'stage': 'compile', 'diagnostic': 'x'}]
        counts, failures = suite.compare(self.reference, observed([], namespaces=failed))
        self.assertEqual((counts['fail'], counts['not-executed'], counts['namespaces-failed']), (0, 2, 1))
        self.assertEqual(failures['assertions'], {})
        self.assertEqual(failures['namespaces'], {'a.b-test': {'stage': 'compile', 'diagnostic': 'x'}})

    def test_missing_assertions_name_the_test_failure(self):
        test_failure = [{'test': 'a.b-test/t', 'stage': 'trap', 'diagnostic': 'fuel'}]
        counts, failures = suite.compare(self.reference, observed([suss_predicate(1)], test_failures=test_failure))
        self.assertEqual(failures['assertions'], {'a.b-test/t#2': 'test:trap'})
        self.assertEqual((counts['pass'], counts['not-executed']), (1, 1))

    def test_unexpected_assertions_and_skips_are_failures(self):
        skip = {'namespace': 'a.b-test', 'symbol': 'x', 'test': None, 'phase': 'load'}
        counts, failures = suite.compare(
            self.reference, observed([suss_predicate(1), suss_predicate(2), suss_predicate(3)], skips=[skip]))
        self.assertEqual(failures['assertions'], {'a.b-test/t#3': 'unexpected-assertion'})
        self.assertEqual(failures['skip-mismatches'], [['a.b-test', 'x', None]])
        self.assertEqual(counts['fail'], 1)

    def test_namespace_coverage_and_duplicates_are_rejected(self):
        with self.assertRaisesRegex(ValueError, 'every suite namespace'):
            suite.compare(self.reference, observed([], namespaces=[{'namespace': 'other', 'status': 'loaded'}]))
        with self.assertRaisesRegex(ValueError, 'duplicate'):
            suite.compare(self.reference, observed([suss_predicate(1), suss_predicate(1)]))


class HarnessData(unittest.TestCase):
    def test_suite_namespace_munging(self):
        expected = {'conj': 'conj', 'not=': 'not-eq', '<=': 'lt-eq', '-': 'minus', "+'": 'plus-squote',
                    'conj!': 'conj-bang', 'any?': 'any-qmark', '/': 'slash', 'pos-int?': 'pos-int-qmark'}
        for symbol, name in expected.items():
            with self.subTest(symbol=symbol):
                self.assertEqual(suite.suite_namespace(symbol), name)
                self.assertIn(f'clojure.core-test.{name}', suite.namespaces())

    def test_legacy_overlap_is_current(self):
        self.assertEqual(json.loads(suite.LEGACY_OVERLAP.read_text()), suite.legacy_overlap())

    def test_every_js_reference_is_classified(self):
        self.assertEqual(suite.verify_interop(), len(suite.interop_occurrences()))

    def test_non_function_heads_match_inventory(self):
        inventory = (suite.ROOT / 'docs/compatibility/cljs-core.edn').read_text()
        macros = set(re.findall(r':name "([^"]+)" :phase :macro :kind :defmacro', inventory))
        functions = set(re.findall(r':name "([^"]+)" :phase :runtime :kind :(?:defn|def)\b', inventory))
        specials = set('if def fn* do let* loop* letfn* throw try recur new set! ns deftype* '
                       'defrecord* . js* & quote case* var ns*'.split())
        source = (suite.HARNESS / 'suss/suss/harness/test_macros.cljc').read_text()
        embedded = re.search(r"\(def non-functions\s+'#\{(.*?)\}\)", source, re.S)[1].split()
        self.assertEqual(sorted(embedded), sorted(specials | (macros - functions) | {'instance?'}))

    def test_vendored_lock_and_namespaces(self):
        suite.verify(__import__('json').loads(suite.LOCK.read_text()))
        self.assertEqual(len(suite.namespaces()), 248)
        self.assertNotIn(suite.PORTABILITY, suite.namespaces())


if __name__ == '__main__':
    unittest.main()
