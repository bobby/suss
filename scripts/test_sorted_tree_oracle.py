import copy
import json
import tempfile
import unittest
from pathlib import Path
import sorted_tree_oracle as oracle

class SortedTreeEvidenceTests(unittest.TestCase):
    def test_closed_source_and_retained_observations(self):
        oracle.check()
        oracle.compare(oracle.ROOT/'docs/compatibility/sorted-tree-collections/evidence/observations.json')

    def test_wrong_values_shapes_effects_and_modes_rejected(self):
        actual=[dict(id=c['id'],mode=c['mode'],**c['expected']) for c in oracle.cases()]
        mutations=[]
        def change(fn):
            rows=copy.deepcopy(actual);fn(rows);mutations.append(rows)
        change(lambda rows:rows.pop())
        change(lambda rows:rows.reverse())
        change(lambda rows:rows[0].update(mode='throw'))
        change(lambda rows:rows[0]['value']['items'].pop())
        change(lambda rows:rows[0]['value']['items'][0]['items'][0].update(bits='0000000000000000'))
        change(lambda rows:rows[0]['trace'].update(bits=True))
        change(lambda rows:rows[0]['value'].update(items='wrong'))
        change(lambda rows:rows[0].update(extra=True))
        i=next(i for i,r in enumerate(actual) if r['value']['tag']=='error')
        change(lambda rows:rows[i]['value'].update(**{'class':'ExceptionInfo'}))
        change(lambda rows:rows[i]['value'].update(message=[True]))
        change(lambda rows:rows[i]['value'].update(message=[]))
        i=next(i for i,r in enumerate(actual) if r['value']['tag']=='vector' and any(x['tag']=='bool' for x in r['value']['items']))
        j=next(j for j,x in enumerate(actual[i]['value']['items']) if x['tag']=='bool')
        change(lambda rows:rows[i]['value']['items'][j].update(value=1))
        with tempfile.TemporaryDirectory() as d:
            p=Path(d)/'actual.json';p.write_text(json.dumps(actual));oracle.compare(p)
            for rows in mutations:
                p.write_text(json.dumps(rows))
                with self.assertRaises(ValueError):oracle.compare(p)
            p.write_text('[{"id":"x","id":"x"}]')
            with self.assertRaises(ValueError):oracle.compare(p)

    def test_string_and_materialization_bounds(self):
        for value in [{'tag':'string','units':[65536]},{'tag':'string','units':[False]}, {'tag':'vector','items':[{'tag':'nil'}]*4097}]:
            with self.assertRaises(ValueError):oracle.observation(value)
        v={'tag':'nil'}
        for _ in range(65):v={'tag':'vector','items':[v]}
        with self.assertRaises(ValueError):oracle.observation(v)

if __name__=='__main__':unittest.main()
