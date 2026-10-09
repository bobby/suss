import copy
import json
import unittest
from sorted_tree_sharing_oracle import expected, validate
from tree_node_oracle import unique


class SharingOracleTests(unittest.TestCase):
    def test_exact_rotation(self):
        validate(expected())

    def test_reject_wrong_shapes_and_reference_facts(self):
        changes = [
            lambda v: v.update(directLeftShared=True),
            lambda v: v.update(nestedLeftShared=False),
            lambda v: v.update(comparatorShared=False),
            lambda v: v.update(rootChanged=False),
            lambda v: v['updated']['left'].update(key=1),
            lambda v: v['updated']['left'].update(**{'class': 'RedNode'}),
            lambda v: v['old'].update(key=True),
            lambda v: v['old'].update(hash=0),
            lambda v: v.update(extra=None),
        ]
        for change in changes:
            value = copy.deepcopy(expected())
            change(value)
            with self.assertRaises(ValueError):
                validate(value)

    def test_duplicate_keys_and_depth_rejected(self):
        with self.assertRaises(ValueError):
            json.loads('{"old":null,"old":null}', object_pairs_hook=unique)
        value = expected()
        child = value['old']
        for _ in range(66):
            child['left'] = copy.deepcopy(expected()['old'])
            child = child['left']
        with self.assertRaises(ValueError):
            validate(value)
