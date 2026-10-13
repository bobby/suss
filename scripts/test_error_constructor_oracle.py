import copy
import json
import tempfile
import unittest
from pathlib import Path
import error_constructor_oracle as oracle

class ErrorConstructorOracleTests(unittest.TestCase):
    def test_closed_cases_driver_and_retained_observations(self):
        oracle.check()
        self.assertEqual(len(oracle.cases()),151)
        oracle.compare(oracle.ROOT/'docs/compatibility/error-constructors/evidence/observations.json')

    def test_reject_order_class_masks_bits_and_extra_keys(self):
        expected=[dict(id=c['id'],mode=c['mode'],**c['expected']) for c in oracle.cases()]
        changes=[lambda a:a.pop(),lambda a:a.reverse(),lambda a:a[0].update(mode='throw'),
            lambda a:a[0]['value'].update(**{'class':'ExceptionInfo'}),
            lambda a:a[0]['value'].update(messageOwn=1),lambda a:a[0]['value'].update(causeOwn=1),
            lambda a:a[0]['value'].update(message=[True]),lambda a:a[0]['value'].update(message=[65536]),
            lambda a:a[0]['value'].update(cause={'tag':'nil'}),
            lambda a:a[0]['trace'].update(bits='8000000000000000'),lambda a:a[0].update(extra=True),
            lambda a:a[0]['value']['cause'].update(extra=None)]
        with tempfile.TemporaryDirectory() as d:
            p=Path(d)/'obs.json'
            for change in changes:
                a=copy.deepcopy(expected);change(a);p.write_text(json.dumps(a))
                with self.assertRaises(ValueError):oracle.compare(p)
            p.write_text('[{"id":"x","id":"x"}]')
            with self.assertRaises(ValueError):oracle.compare(p)

    def test_original_87_observations_preserved(self):
        historical=oracle.read(oracle.ROOT/'docs/compatibility/error-constructors/evidence/observations-before-mutation-87.json')
        current=oracle.read(oracle.ROOT/'docs/compatibility/error-constructors/evidence/observations.json')
        self.assertEqual(len(historical),87)
        self.assertEqual(current[:87],historical)

    def test_descriptor_flags_and_shape_rejected(self):
        valid={'tag':'data-descriptor','value':{'tag':'undefined'},'writable':False,'enumerable':False,'configurable':True}
        oracle.observation(valid)
        for key in ('writable','enumerable','configurable'):
            bad=copy.deepcopy(valid);bad[key]=1
            with self.assertRaises(ValueError):oracle.observation(bad)
        bad=copy.deepcopy(valid);bad['extra']=True
        with self.assertRaises(ValueError):oracle.observation(bad)

    def test_depth_rejected(self):
        v={'tag':'undefined'}
        for _ in range(66):v={'tag':'error','class':'Error','message':[],'messageOwn':False,'causeOwn':True,'cause':v}
        with self.assertRaises(ValueError):oracle.observation(v)
