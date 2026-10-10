import copy
import json
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch
import portable_oracle
import record_graph_runtime_oracle as oracle

class RuntimeGraphOracle(unittest.TestCase):
    def setUp(self): self.original=json.loads(oracle.CORPUS.read_text())
    def load(self,corpus):
        with tempfile.TemporaryDirectory() as directory:
            path=Path(directory)/'corpus.json';path.write_text(json.dumps(corpus))
            with patch.object(oracle,'CORPUS',path): return oracle.corpus()
    def test_closed_ordered_all_arities(self): self.assertEqual(len(oracle.corpus()['cases']),84)
    def test_bad_ids_and_forms_fail_closed(self):
        for change in ('missing','reordered','duplicate','unknown','extra-form'):
            corpus=copy.deepcopy(self.original)
            if change=='missing':corpus['cases'].pop()
            elif change=='reordered':corpus['cases'].reverse()
            elif change=='duplicate':corpus['cases'][1]=copy.deepcopy(corpus['cases'][0])
            elif change=='unknown':corpus['cases'][0]['id']='unknown'
            else:corpus['cases'][0]['source']+=' nil'
            with self.subTest(change=change),self.assertRaises(ValueError):self.load(corpus)
    def test_raw_bool_number_and_trace_order_rejected(self):
        document={'schema':1,'upstream':self.original['upstream'],'cases':[{'id':case['id'],'value':case['expected']}for case in self.original['cases']]}
        for change in ('bool-number','trace-order'):
            actual=copy.deepcopy(document)
            if change=='bool-number':actual['cases'][0]['value']['value']=0
            else:actual['cases'][2]['value']['items'][1]['items'].reverse()
            with tempfile.TemporaryDirectory() as directory:
                path=Path(directory)/'actual.json';path.write_text(json.dumps(actual))
                with patch.object(oracle,'OBSERVATIONS',path),self.subTest(change=change),self.assertRaises(ValueError):oracle.compare()
    def test_global_paths_restore_after_failure(self):
        saved=portable_oracle.CORPUS,portable_oracle.OBSERVATIONS
        with patch.object(oracle,'CORPUS',Path('/missing-runtime-graph-corpus')):
            with self.assertRaises(FileNotFoundError):oracle.corpus()
        self.assertEqual(saved,(portable_oracle.CORPUS,portable_oracle.OBSERVATIONS))
