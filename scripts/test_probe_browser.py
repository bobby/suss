import copy
import unittest
from probe_browser import FEATURES, REQUIRED_CHECKS, JCO_CHECKS, validate_result


def passing(jco=False):
    return {'status': 'passed', 'features': list(FEATURES), 'checks': list(REQUIRED_CHECKS),
            'jco': {'status': 'passed', 'checks': list(JCO_CHECKS)} if jco else {'status': 'not-requested'}}


class BrowserResultTests(unittest.TestCase):
    def test_complete_direct_and_optional_packaging_results(self):
        validate_result(passing())
        validate_result(passing(True), require_jco=True)

    def test_missing_and_unknown_results_fail(self):
        for value in (None, [], {}, {'status': 'passed'}, {'status': 'unknown'}, {'status': 'failed'}):
            with self.subTest(value=value), self.assertRaises(ValueError):
                validate_result(value)

    def test_missing_feature_or_acceptance_check_fails(self):
        for key in ('features', 'checks'):
            value = passing()
            value[key].pop()
            with self.subTest(key=key), self.assertRaises(ValueError):
                validate_result(value)

    def test_jco_never_defaults_to_success(self):
        for value in (passing(), dict(passing(), jco={'status': 'passed'})):
            with self.subTest(value=value), self.assertRaises(ValueError):
                validate_result(value, require_jco=True)
        with self.assertRaises(ValueError):
            validate_result(passing(True))

    def test_jco_missing_execution_check_fails(self):
        value = copy.deepcopy(passing(True))
        value['jco']['checks'].pop()
        with self.assertRaises(ValueError):
            validate_result(value, require_jco=True)


if __name__ == '__main__':
    unittest.main()
