"""Verify the complete primary some-fn form and its explicit bootstrap adaptation."""
from pathlib import Path
import difflib,hashlib,json,sys,zipfile
ROOT=Path(__file__).resolve().parents[2];BASE=Path(__file__).resolve().parent
sha=lambda raw:hashlib.sha256(raw).hexdigest()
source_record=json.loads((BASE/'source-provenance.json').read_text())
jar=Path(source_record['jar']);assert sha(jar.read_bytes())==source_record['jar_sha256']
with zipfile.ZipFile(jar) as archive:primary_source=archive.read(source_record['entry'])
assert sha(primary_source)==source_record['entry_sha256']
original=(BASE/'upstream-some-fn.clj').read_bytes()
assert sha(original)==source_record['retained_forms']['results/m12-core-dependencies/upstream-some-fn.clj']
assert original.rstrip(b'\n') in primary_source
adaptation=next(row for row in json.loads((BASE/'adaptation.json').read_text()) if row['name']=='some-fn')
adapted=(BASE/'some-fn-adapted.sus').read_bytes()
assert sha(adapted)==adaptation['adapted_sha256'] and sha(original)==adaptation['source_sha256']
patch=''.join(difflib.unified_diff(original.decode().splitlines(True),adapted.decode().splitlines(True),fromfile='clojure.core/some-fn',tofile='suss.core/some-fn')).encode()
assert patch==(BASE/'some-fn.patch').read_bytes() and sha(patch)==adaptation['patch_sha256']
core=(ROOT/'docs/compatibility/bootstrap/exact-integers.sus').read_bytes()
start=core.index(b';; BEGIN Clojure some-fn\n')+len(b';; BEGIN Clojure some-fn\n')
end=core.index(b';; END Clojure some-fn\n',start)
assert core[start:end]==adapted,'the complete imported arities/doc/metadata must remain byte-preserved'
original_rows=json.loads((BASE/'cases.json').read_text());probe_rows=json.loads((BASE/'probe-cases.json').read_text())
rows=[row for row in probe_rows if row['name'].startswith('some-fn-')];assert len(rows)==364
for row in rows:
 primary=next(x for x in original_rows if x['name']==row['name'])
 assert row['expected']==primary['expected'] and row['original_expression']==primary['expression']
assert sum(row['expression']!=row['original_expression'] for row in rows)==0
equivalence=json.loads((BASE/'probe-equivalence-command.json').read_text());assert equivalence['exit_code']==0
effects=json.loads((BASE/'some-fn-effects/cases.json').read_text())
effect_command=json.loads((BASE/'some-fn-effects/command.json').read_text())
assert len(effects)==effect_command['rows']==8 and effect_command['exit_code']==0
assert all(sha(Path(p).read_bytes())==h for p,h in effect_command['jars'].items())
assert sha((BASE/'some-fn-effects/oracle.clj').read_bytes())==effect_command['source_sha256']
assert sha((BASE/'some-fn-effects/stdout.log').read_bytes())==effect_command['stdout_sha256']
assert [r['expected'] for r in effects]==[json.loads(line) for line in (BASE/'some-fn-effects/stdout.log').read_text().splitlines()]
files=['docs/compatibility/bootstrap/exact-integers.sus','docs/compatibility/core-import.json','runtime/core-import/manifest.json','runtime/core-import/suss/core.sus','crates/suss-cli/tests/clojure_some_fn.rs','crates/suss-compile/tests/support/pinned_some_fn_bodies.rs','results/m12-core-dependencies/upstream-some-fn.clj','results/m12-core-dependencies/some-fn-adapted.sus','results/m12-core-dependencies/some-fn.patch','results/m12-core-dependencies/adaptation.json','results/m12-core-dependencies/NOTICE','results/m12-core-dependencies/cases.json','results/m12-core-dependencies/probe-cases.json','results/m12-core-dependencies/probe-equivalence-command.json']
record={'primary':{'upstream':source_record['upstream'],'jar_sha256':source_record['jar_sha256'],'entry':source_record['entry'],'entry_sha256':source_record['entry_sha256'],'complete_form_sha256':sha(original)},'adaptation':adaptation,'license':{'id':'EPL-1.0','notice':'results/m12-core-dependencies/NOTICE','text':'runtime/core-import/epl-v10.html'},'controls':{'primary_some_fn_controls':364,'pure_fixture_materializations':0,'qualification':'All364 some-fn primary expressions and expected values are unchanged; no local replacement is injected.'},'supplemental_controls':{'count':8,'fixture_sha256':sha((BASE/'some-fn-effects/cases.json').read_bytes()),'command_sha256':sha((BASE/'some-fn-effects/command.json').read_bytes())},'files':{p:sha((ROOT/p).read_bytes()) for p in files},'limits':'some-fn import only. The assert macro dependency, map-indexed and real compiled eval require separate evidence. Fixtures/provenance do not establish a native, full-body, workspace or numeric acceptance pass.'}
path=ROOT/'docs/compatibility/bootstrap/clojure-some-fn.json'
if '--record' in sys.argv:path.write_text(json.dumps(record,indent=2)+'\n')
else:assert json.loads(path.read_text())==record,'some-fn provenance differs'
print('Complete Clojure some-fn source, adaptation and fixtures '+('recorded' if '--record' in sys.argv else 'verified'))
