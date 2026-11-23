"""Verify retained primary forms, exact adaptations and their shipped bytes."""
from pathlib import Path
import difflib,hashlib,json,sys,zipfile
ROOT=Path(__file__).resolve().parents[2];BASE=Path(__file__).resolve().parent
sha=lambda raw:hashlib.sha256(raw).hexdigest()
p=json.loads((BASE/'source-provenance.json').read_text())
assert all(sha(Path(path).read_bytes())==h for path,h in p['jars'].items())
jar=next(Path(path) for path in p['jars'] if path.endswith('/clojure-1.12.6.jar'))
with zipfile.ZipFile(jar) as z:source=z.read(p['entry'])
assert sha(source)==p['entry_sha256']
core=(ROOT/'docs/compatibility/bootstrap/exact-integers.sus').read_bytes()
forms={}
for name in ['rand','rand-int']:
 original=(BASE/('upstream-'+name+'.clj')).read_bytes()
 assert sha(original)==p['complete_forms'][name] and original.rstrip() in source
 adapted=(BASE/(name+'-adapted.sus')).read_bytes()
 patch=''.join(difflib.unified_diff(original.decode().splitlines(True),adapted.decode().splitlines(True),fromfile='clojure.core/'+name,tofile='suss.core/'+name)).encode()
 assert patch==(BASE/(name+'.patch')).read_bytes()
 marker=(';; BEGIN Clojure '+name+'\n').encode();end=(';; END Clojure '+name+'\n').encode()
 assert core.count(marker)==1 and core.split(marker,1)[1].split(end,1)[0]==adapted
 forms[name]={'original_sha256':sha(original),'adapted_sha256':sha(adapted),'patch_sha256':sha(patch)}
command=json.loads((BASE/'command.json').read_text());assert command['exit_code']==0
assert sha((BASE/'oracle.clj').read_bytes())==command['source_sha256']
assert sha((BASE/'stdout.log').read_bytes())==command['stdout_sha256']
rows=json.loads((BASE/'controlled-transformations.json').read_text());assert len(rows)==126
assert [r['expected'] for r in rows]==[json.loads(line) for line in (BASE/'stdout.log').read_text().splitlines()[:126]]
paths=['docs/compatibility/bootstrap/exact-integers.sus','docs/compatibility/core-import.json','runtime/core-import/suss/core.sus','runtime/core-import/manifest.json','crates/suss-compile/src/runtime_abi/random.rs','crates/suss-compile/src/portable/aot/random.rs','crates/suss-compile/src/portable_session/random_tests.rs','crates/suss-compile/tests/runtime_random.rs','crates/suss-compile/tests/portable_random.rs']
record={'primary':p,'forms':forms,'license':{'id':'EPL-1.0','text':'runtime/core-import/epl-v10.html'},'adaptation':'Retain both public arities, doc/metadata and multiply-then-checked-int steps. Replace Math.random with a private execution-time entropy intrinsic. Lexically capture the existing checked multiplication/int function values so core legacy coercing macros do not change the primary Object overloads. Standard random imports are selected explicitly from the resolved world and encoded with the generic WIT type encoder. Native sessions install OS entropy. No constant or compilation-time seed fallback.','controlled_primary_transformations':126,'qualification':'The primary rows isolate the deterministic transformations at fixed draws, not JVM random-stream goldens. Provider feasibility is separate from production/public API acceptance; fresh native receipts are required.','files':{path:sha((ROOT/path).read_bytes()) for path in paths}}
path=ROOT/'docs/compatibility/bootstrap/clojure-random.json'
if '--record' in sys.argv:path.write_text(json.dumps(record,indent=2)+'\n')
else:assert json.loads(path.read_text())==record
print('Clojure random source/adaptation/primary '+('recorded' if '--record' in sys.argv else 'verified'))
