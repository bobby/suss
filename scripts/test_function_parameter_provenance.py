import copy
import json
import unittest
from check_function_parameter_provenance import DIRECTORY, verify

class FunctionParameterProvenance(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.provenance = json.loads((DIRECTORY / 'provenance.json').read_text())

    def test_complete_closed_retention(self):
        self.assertEqual(verify(self.provenance), 5)

    def test_missing_reordered_and_duplicate_dependencies_rejected(self):
        for mutation in ('missing', 'reordered', 'duplicate'):
            with self.subTest(mutation=mutation):
                data = copy.deepcopy(self.provenance)
                if mutation == 'missing':
                    data['forms'].pop()
                elif mutation == 'reordered':
                    data['forms'][0], data['forms'][1] = data['forms'][1], data['forms'][0]
                else:
                    data['forms'][1] = copy.deepcopy(data['forms'][0])
                with self.assertRaises(ValueError):
                    verify(data)

    def test_false_hash_range_context_or_execution_claim_rejected(self):
        for key, value in [('sha256', '0' * 64), ('end-line', 1), ('reader-context', [':clj']), ('status', 'executed')]:
            with self.subTest(key=key):
                data = copy.deepcopy(self.provenance)
                data['forms'][0][key] = value
                with self.assertRaises(ValueError):
                    verify(data)

if __name__ == '__main__':
    unittest.main()
