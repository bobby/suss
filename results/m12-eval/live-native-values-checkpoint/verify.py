#!/usr/bin/env python3
"""Verify exact source-conserved native live-value commands and their limits."""
from pathlib import Path
import gzip,hashlib,json,re,subprocess
here=Path(__file__).resolve().parent
root=here.parents[2]
sha=lambda data:hashlib.sha256(data).hexdigest()
s=json.loads((here/'summary.json').read_text())
for name,digest in s['archive_hashes'].items():assert sha((here/name).read_bytes())==digest,name
phases=['before','typecheck','working','arrays-and-frontends','storage-guarded']
states={p:json.loads(gzip.decompress((here/p/'commands.json.gz').read_bytes())) for p in phases}
for phase,state in states.items():
 assert state['complete'] and state['inputs_unchanged'] and state['binaryen_unchanged'],phase
 assert sha(gzip.decompress((here/phase/'runner.py.gz').read_bytes()))==state['runner_sha256'],phase
 for row in state['commands']:assert row['complete'],(phase,row['name'])
 for name,digest in state['inputs'].items():
  overlay=here/phase/'overlay'/(name+'.gz')
  if overlay.exists():
   assert sha(gzip.decompress(overlay.read_bytes()))==digest,(phase,name)
   continue
  if (root/name).is_file() and sha((root/name).read_bytes())==digest:continue
  previous=subprocess.run(['git','show',state['head']+':'+name],cwd=root,capture_output=True)
  assert previous.returncode==0 and sha(previous.stdout)==digest,('unreconstructed',phase,name)
assert [r['exit_code'] for r in states['before']['commands']]==[101]
assert [r['exit_code'] for r in states['typecheck']['commands']]==[101]
assert [r['exit_code'] for r in states['working']['commands']]==[0,0,0,0,101]
assert [(r['name'],r['exit_code']) for r in states['arrays-and-frontends']['commands'] if r['exit_code']]==[('private-native-capability',101),('full20-source-values',101),('ownership-library',101)]
current=states['storage-guarded']
assert len(current['commands'])==s['commands']==10
assert [(r['name'],r['exit_code']) for r in current['commands'] if r['exit_code']]==[('full20-source-values',101)]
for row in current['commands']:
 text=gzip.decompress((here/'storage-guarded'/(row['name']+'.log.gz')).read_bytes()).decode()
 count=sum(map(int,re.findall(r'test result: ok\. (\d+) passed; 0 failed;',text)))
 assert count==s['test_counts'][row['name']],row['name']
text=gzip.decompress((here/'storage-guarded/full20-source-values.log.gz').read_bytes()).decode()
for phase,count in s['full20_matches'].items():assert len(re.findall('isolated-source-primary-match '+phase+' ',text))==count==19
assert 'Unresolved Macro name ns-interns' in text
for name,digest in current['inputs'].items():
 expected=s['prose_delta'].get(name,{}).get('published',digest)
 assert sha((root/name).read_bytes())==expected,name
 if name in s['prose_delta']:assert s['prose_delta'][name]['tested']==digest
for name in s['source_files']:
 data=gzip.decompress((here/'storage-guarded/overlay'/(name+'.gz')).read_bytes())
 assert data==(root/name).read_bytes(),name
paths=set(subprocess.check_output(['git','ls-files','-co','--exclude-standard'],cwd=root,text=True).splitlines())
assert all(not(root/p).is_file() or p.startswith('results/') or p in current['inputs'] for p in paths)
print('Verified ten current terminals, exact current/negative source and runner reconstructions, 19/20 observations per phase, and the explicit documentation delta. The ns-interns aggregate remains failing; no full issue or promotion claim.')
