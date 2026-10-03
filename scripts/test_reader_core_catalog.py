import unittest
from reader_core_catalog import names_from_source


class ReaderCoreCatalogTests(unittest.TestCase):
    def test_module_declarations_protocol_methods_and_constructors(self):
        source = '(def x 1) (defprotocol P "doc" (^number -method [this])) (deftype T [x]) (defrecord R [x]) (defn f [] (def hidden 1))'
        self.assertEqual(names_from_source(source, 'runtime'),
                         {'x', 'P', '-method', 'T', '->T', 'R', '->R', 'map->R', 'f'})

    def test_macro_helpers_are_not_runtime_names_and_templates_are_not_declarations(self):
        source = '(defn helper [] nil) (defmacro m [x] `(def fake ~x))'
        self.assertEqual(names_from_source(source, 'macro'), {'m'})

    def test_discard_and_quoted_declarations_are_not_catalog_facts(self):
        source = "#_(def discarded 1) '(def quoted 1) (def visible 1)"
        self.assertEqual(names_from_source(source, 'runtime'), {'visible'})


if __name__ == '__main__':
    unittest.main()
