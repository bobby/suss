import copy
import json
import tempfile
import unittest
from pathlib import Path
import tree_node_raw_oracle as raw

class RawTreeEvidenceTests(unittest.TestCase):
    def test_current_closed_cases_and_driver(self):
        raw.check()
        self.assertEqual(len(raw.cases()), 52)
        raw.compare(raw.ROOT / "docs/compatibility/sorted-tree-nodes/evidence/raw-observations/observations.json")

    def test_raw_results_are_exact_closed_values(self):
        actual = [dict(id=c['id'], mode=c['mode'], **c['expected']) for c in raw.cases()]
        mutations = []
        def mutation(change):
            rows = copy.deepcopy(actual); change(rows); mutations.append(rows)
        mutation(lambda rows: rows.pop())
        mutation(lambda rows: rows.reverse())
        mutation(lambda rows: rows[0].update(mode='throw'))
        mutation(lambda rows: rows[0]['value'].update(**{'class':'RedNode'}))
        mutation(lambda rows: rows[0]['value']['key'].update(bits='0000000000000000'))
        mutation(lambda rows: rows[0]['value'].update(left={'tag':'bool','value':True}))
        mutation(lambda rows: rows[0]['value'].update(hash={'tag':'unknown'}))
        mutation(lambda rows: rows[0]['trace'].update(bits=True))
        mutation(lambda rows: rows[0].update(extra=True))
        error_index = next(i for i,r in enumerate(actual) if r['value']['tag']=='error')
        mutation(lambda rows: rows[error_index]['value'].update(**{'class':'ExceptionInfo'}))
        mutation(lambda rows: rows[error_index]['value'].update(message=[True]))
        mutation(lambda rows: rows[error_index]['value'].update(message=[65536]))
        mutation(lambda rows: rows[error_index]['value'].update(message=[]))
        with tempfile.TemporaryDirectory() as d:
            path=Path(d)/'observations.json'
            path.write_text(json.dumps(actual)); raw.compare(path)
            for rows in mutations:
                path.write_text(json.dumps(rows))
                with self.assertRaises(ValueError): raw.compare(path)
            path.write_text('[{"id":"x","id":"x"}]')
            with self.assertRaises(ValueError): raw.compare(path)

    def test_host_decoder_depth_bound(self):
        v={'tag':'nil'}
        for _ in range(65):v={'tag':'reduced','value':v}
        with self.assertRaises(ValueError):raw.observation(v)

if __name__ == '__main__': unittest.main()
