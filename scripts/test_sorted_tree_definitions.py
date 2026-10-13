"""Whole declaration expansion guards for the retained sorted source group."""
import json
import unittest
from pathlib import Path
from cljs_inventory import Scanner
from sorted_tree_adaptations import lower_definition

ROOT = Path(__file__).resolve().parents[1]

class WholeDefinitionTests(unittest.TestCase):
    def test_original_documentation_signatures_and_bodies_survive(self):
        receipt = json.loads((ROOT/'docs/compatibility/sorted-tree-collections/provenance.json').read_text())
        count = 0
        for entry in receipt['forms']:
            raw = (ROOT/entry['retained']).read_text().rstrip('\n')
            original = Scanner(raw).all()[0]
            if original.children[0].text not in ('defn', 'defn-'):
                continue
            count += 1
            lowered = lower_definition(raw, entry['name'])
            top = Scanner(lowered).all()[0]
            self.assertEqual(top.children[0].text, 'def')
            self.assertEqual(top.children[-1].children[0].text, 'fn')
            position = 2
            if original.children[position].kind == 'string':
                doc = original.children[position]
                self.assertIn(':doc '+raw[doc.start:doc.end], lowered)
                position += 1
            expected = raw[original.children[position].start:original.end-1]
            fn = top.children[-1]
            actual = lowered[fn.children[1].start:fn.end-1]
            self.assertEqual(actual, expected, entry['name'])
            self.assertEqual(':private true' in lowered, original.children[0].text == 'defn-')
            self.assertIn(':arglists', lowered)
        self.assertEqual(count, 23)

    def test_unreviewed_attribute_shapes_are_rejected(self):
        for source in ['(defn x {:added true} [a] a)',
                       '(defn ^:dynamic x [a] a)',
                       '(defn x ([a] a) {:added true})',
                       '(defn x)']:
            with self.assertRaises(ValueError):
                lower_definition(source, 'x')

if __name__ == '__main__':
    unittest.main()
