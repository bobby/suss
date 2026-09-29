import unittest
from cljs_inventory import Scanner, declarations


class InventoryScannerTests(unittest.TestCase):
    def defs(self, source):
        return [(kind, name, context) for f in Scanner(source).all()
                for kind, name, _, context in declarations(f)]

    def test_metadata_and_lexical_delimiters(self):
        source = '(defn ^{:doc "(fake)"} f [x] [\\) #"[)]" #js {}]) ; (def bad)\n(def ^:private g 1)'
        self.assertEqual([x[1] for x in self.defs(source)], ['f', 'g'])

    def test_no_templates_or_discarded_definitions(self):
        source = "#_(def removed 1) '(def quoted 1) (defmacro m [] `(def generated 1))"
        self.assertEqual([x[1] for x in self.defs(source)], ['m'])

    def test_reader_branches_and_module_containers(self):
        source = '#?(:clj (do (core/defmacro m [] nil)) :cljs (defn m [] nil))'
        self.assertEqual(self.defs(source), [('defmacro', 'm', (':clj', 'do')),
                                            ('defn', 'm', (':cljs',))])
        self.assertEqual([x[1] for x in self.defs('#?@(:cljs [(def a 1) (def b 2)])')], ['a', 'b'])

    def test_prime_in_symbol_is_not_a_quote(self):
        self.assertEqual([x[1] for x in self.defs("(def in' 1)")], ["in'"])

    def test_unbalanced_source_fails(self):
        for source in ('(def a', '(def a]', '(def a "oops)', ')'):
            with self.assertRaises(ValueError):
                Scanner(source).all()


if __name__ == '__main__':
    unittest.main()
