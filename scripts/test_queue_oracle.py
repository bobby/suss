"""Reject missing, reordered, mistyped and concealed queue observations."""
import copy
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import queue_oracle as oracle

class QueueOracleTests(unittest.TestCase):
    def test_complete_observations_and_mutations(self):
        cases=oracle.load()
        values=[{'id':c['id'],'value':c['expected']['value']} for c in cases]
        with tempfile.TemporaryDirectory() as directory:
            f=Path(directory)/'f.json';i=Path(directory)/'i.json'
            with patch.object(oracle,'FOUNDATIONS',f),patch.object(oracle,'ITERATOR',i):
                f.write_text(json.dumps(values));i.write_text(json.dumps(oracle.TRACE));oracle.compare()
                for changed in [values[:-1],list(reversed(values)),[dict(values[0],value=1)]+values[1:],[dict(values[0],extra=False)]+values[1:]]:
                    f.write_text(json.dumps(changed))
                    with self.assertRaises(ValueError):oracle.compare()
                f.write_text(json.dumps(values))
                for index,value in [(0,1),(2,True),(8,{'error':False,'message':'No such element'}),(9,{'error':True,'message':'different'})]:
                    changed=copy.deepcopy(oracle.TRACE);changed[index]=value;i.write_text(json.dumps(changed))
                    with self.assertRaises(ValueError):oracle.compare()
    def test_duplicate_nonfinite_size_and_trailing_input(self):
        with tempfile.TemporaryDirectory() as directory:
            p=Path(directory)/'input.json'
            for raw in ['{"value":true,"value":false}','[NaN]','{} {}',' '*(1<<20)+'{}']:
                p.write_text(raw)
                with self.assertRaises(ValueError):oracle.read(p)
    def test_typed_recursive_comparison(self):
        self.assertFalse(oracle.same(True,1));self.assertFalse(oracle.same(1,True))
        self.assertFalse(oracle.same({'error':True,'extra':None},{'error':True}))
        self.assertTrue(oracle.same([1.0,False],[1,False]))

if __name__=='__main__':unittest.main()
