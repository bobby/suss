"""Verify the complete primary min-key form and its explicit bootstrap adaptation."""
from pathlib import Path
import difflib,hashlib,json,sys,zipfile
ROOT=Path(__file__).resolve().parents[2];BASE=Path(__file__).resolve().parent
sha=lambda raw:hashlib.sha256(raw).hexdigest()
source_record=json.loads((BASE/'source-provenance.json').read_text())
jar=Path(source_record['jar']);assert sha(jar.read_bytes())==source_record['jar_sha256']
with zipfile.ZipFile(jar) as archive:primary_source=archive.read(source_record['entry'])
assert sha(primary_source)==source_record['entry_sha256']
original=(BASE/'upstream-min-key.clj').read_bytes()
assert sha(original)==source_record['retained_forms']['results/m12-core-dependencies/upstream-min-key.clj']
assert original.rstrip(b'\n') in primary_source
adaptation=next(row for row in json.loads((BASE/'adaptation.json').read_text()) if row['name']=='min-key')
adapted=(BASE/'min-key-adapted.sus').read_bytes()
assert sha(adapted)==adaptation['adapted_sha256'] and sha(original)==adaptation['source_sha256']
patch=''.join(difflib.unified_diff(original.decode().splitlines(True),adapted.decode().splitlines(True),fromfile='clojure.core/min-key',tofile='suss.core/min-key')).encode()
assert patch==(BASE/'min-key.patch').read_bytes() and sha(patch)==adaptation['patch_sha256']
core=(ROOT/'docs/compatibility/bootstrap/exact-integers.sus').read_bytes()
start=core.index(b';; BEGIN Clojure min-key\n')+len(b';; BEGIN Clojure min-key\n')
end=core.index(b';; END Clojure min-key\n',start)
assert core[start:end]==adapted,'the complete imported arities/doc/metadata must remain byte-preserved'
original_rows=json.loads((BASE/'cases.json').read_text());probe_rows=json.loads((BASE/'probe-cases.json').read_text())
rows=[row for row in probe_rows if row['name'].startswith('min-key-')];assert len(rows)==32
for row in rows:
 primary=next(x for x in original_rows if x['name']==row['name'])
 assert row['expected']==primary['expected'] and row['original_expression']==primary['expression']
assert sum(row['expression']!=row['original_expression'] for row in rows)==6
equivalence=json.loads((BASE/'probe-equivalence-command.json').read_text());assert equivalence['exit_code']==0
files=['docs/compatibility/bootstrap/exact-integers.sus','docs/compatibility/core-import.json','runtime/core-import/manifest.json','runtime/core-import/suss/core.sus','crates/suss-cli/tests/clojure_min_key.rs','crates/suss-compile/tests/support/pinned_min_key_body.rs','results/m12-core-dependencies/upstream-min-key.clj','results/m12-core-dependencies/min-key-adapted.sus','results/m12-core-dependencies/min-key.patch','results/m12-core-dependencies/NOTICE','results/m12-core-dependencies/cases.json','results/m12-core-dependencies/probe-cases.json','results/m12-core-dependencies/probe-equivalence-command.json']
record={'primary':{'upstream':source_record['upstream'],'jar_sha256':source_record['jar_sha256'],'entry':source_record['entry'],'entry_sha256':source_record['entry_sha256'],'complete_form_sha256':sha(original)},'adaptation':adaptation,'license':{'id':'EPL-1.0','notice':'results/m12-core-dependencies/NOTICE','text':'runtime/core-import/epl-v10.html'},'controls':{'primary_min_key_controls':32,'pure_fixture_materializations':6,'qualification':'Only pure map-indexed candidate-map constructors are materialized; fresh JVM execution checks all396 combined probe values against untouched primary controls. No min-key/key-function body changes.'},'files':{p:sha((ROOT/p).read_bytes()) for p in files},'limits':'min-key only. some-fn, map-indexed and real compiled eval keep their separate gaps. Fixtures/provenance do not establish passing native or complete numeric acceptance.'}
path=ROOT/'docs/compatibility/bootstrap/clojure-min-key.json'
if '--record' in sys.argv:path.write_text(json.dumps(record,indent=2)+'\n')
else:assert json.loads(path.read_text())==record,'min-key provenance differs'
print('Complete Clojure min-key source, adaptation and fixtures '+('recorded' if '--record' in sys.argv else 'verified'))
