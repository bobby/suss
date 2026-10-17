"""Compare pinned results with explicit native fixture expectations, no Suss run."""
import hashlib,json,pathlib,struct
p=pathlib.Path(__file__).parent
root=pathlib.Path('/home/bobby/Work/code/bobby/suss-m4-when-let')
inputs=json.loads((p/'inputs.json').read_text())
assert hashlib.sha256((root/inputs['native_fixture']).read_bytes()).hexdigest()==inputs['native_fixture_sha256']
def encode(value):
    if isinstance(value,list):return {'tag':'vector','items':[encode(x) for x in value]}
    return {'tag':'f64','bits':struct.pack('>d',float(value)).hex()}
expected={'schema':1,'upstream':inputs['upstream'],'values':[encode([17,19,[17,19],23,19,59,77,[1,4,2,3,5]]),encode([[11,7,18],6])]}
(p/'expected.json').write_text(json.dumps(expected,indent=2)+'\n')
for attempt in ['run1','run2']:
    actual=json.loads((p/(attempt+'-observations.json')).read_text())
    assert actual==expected,(attempt,actual,expected)
record={'schema':1,'status':'match','fixtures':2,'fresh_runs':2,'native_fixture_sha256':inputs['native_fixture_sha256'],'expected_sha256':hashlib.sha256((p/'expected.json').read_bytes()).hexdigest(),'observations':{attempt:hashlib.sha256((p/(attempt+'-observations.json')).read_bytes()).hexdigest() for attempt in ['run1','run2']},'scope':'Pinned output equals explicit native test expected values for both guests. Native execution is separate; no compiler metering or source authority claim.'}
(p/'comparison.json').write_text(json.dumps(record,indent=2)+'\n')
print(json.dumps(record))
