#!/usr/bin/env python3
"""Verify frozen current terminals and their exact source/prose publication delta."""
from pathlib import Path
import gzip,hashlib,json,re,subprocess
here=Path(__file__).resolve().parent
root=here.parents[2]
sha=lambda b:hashlib.sha256(b).hexdigest()
summary=json.loads((here/'summary.json').read_text())
for path,digest in summary['archive_hashes'].items():
 assert sha((here/path).read_bytes())==digest,path
def state(phase):
 return json.loads(gzip.decompress((here/phase/'commands.json.gz').read_bytes()))
current=state('cache-evicted')
assert current['complete'] and current['exit_code']==0 and current['inputs_unchanged'] and current['binaryen_unchanged']
assert len(current['commands'])==summary['current_commands']==8
for row in current['commands']:
 assert row['complete'] and row['exit_code']==0,row
 log=gzip.decompress((here/'cache-evicted'/(row['name']+'.log.gz')).read_bytes()).decode()
 count=sum(map(int,re.findall(r'test result: ok\. (\d+) passed; 0 failed;',log)))
 assert count==summary['test_counts'][row['name']],row['name']
for path,digest in current['inputs'].items():
 expected=summary['prose_delta'].get(path,{}).get('published',digest)
 assert sha((root/path).read_bytes())==expected,path
 if path in summary['prose_delta']:assert digest==summary['prose_delta'][path]['tested']
for path in summary['source_files']:
 tested=gzip.decompress((here/'overlay'/(path+'.gz')).read_bytes())
 assert sha(tested)==current['inputs'][path],path
 assert tested==(root/path).read_bytes(),path
# Every negative input is either unchanged at publication or exactly at its
# recorded base. Do not infer negative source from a different working overlay.
negative=state('before')
assert negative['complete'] and negative['exit_code']==101 and negative['inputs_unchanged'] and negative['binaryen_unchanged']
assert len(negative['commands'])==1 and negative['commands'][0]['exit_code']==101
for path,digest in negative['inputs'].items():
 if (root/path).is_file() and sha((root/path).read_bytes())==digest:continue
 blob=subprocess.run(['git','show',negative['head']+':'+path],cwd=root,capture_output=True)
 assert blob.returncode==0 and sha(blob.stdout)==digest,('negative reconstruction',path)
assert sha(gzip.decompress((here/'runners/run-nested-native-metadata-first.py.gz').read_bytes()))==negative['runner_sha256']
assert sha(gzip.decompress((here/'runners/run-nested-native-metadata.py.gz').read_bytes()))==current['runner_sha256']
guarded=state('root-guarded')
assert guarded['complete'] and guarded['exit_code']==101 and guarded['inputs_unchanged'] and guarded['binaryen_unchanged']
assert len(guarded['commands'])==8
assert [(r['name'],r['exit_code']) for r in guarded['commands'] if r['exit_code']] == [('private-native-capability',101),('ownership-library',101)]
for path in summary['source_files']:
 blob=gzip.decompress((here/'root-guarded-overlay'/(path+'.gz')).read_bytes())
 assert sha(blob)==guarded['inputs'][path],path
for path,digest in guarded['inputs'].items():
 if path in summary['source_files']:continue
 if (root/path).is_file() and sha((root/path).read_bytes())==digest:continue
 blob=subprocess.run(['git','show',guarded['head']+':'+path],cwd=root,capture_output=True)
 assert blob.returncode==0 and sha(blob.stdout)==digest,('guarded reconstruction',path)
assert sha(gzip.decompress((here/'runners/run-nested-native-metadata-root-guarded.py.gz').read_bytes()))==guarded['runner_sha256']
first=state('working')
assert first['complete'] and first['exit_code']==0 and first['inputs_unchanged'] and first['binaryen_unchanged']
assert all(row['complete'] and row['exit_code']==0 for row in first['commands'])
assert first['runner_sha256']==negative['runner_sha256']
# New production inputs after the frozen terminal require new validation.
paths=set(subprocess.check_output(['git','ls-files','-co','--exclude-standard'],cwd=root,text=True).splitlines())
assert all(not (root/path).is_file() or path.startswith('results/') or path in current['inputs'] for path in paths)
print('Verified eight current terminals, exact source/fixture inputs, negative reconstruction and explicit documentation delta. Intermediate historical source has the stated limit; full Var/Namespace acceptance and promotion remain unproved.')
