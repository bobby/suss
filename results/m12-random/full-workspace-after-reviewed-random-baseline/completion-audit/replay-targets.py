"""Replay the exact random/some-fn workspace target coverage from archives.

The original durable recorder supplies Cargo's actual exit and unchanged-input/
HEAD observations. This replay verifies that receipt and every native/doc
result against archived metadata and fingerprints, without live build files.
"""
from pathlib import Path
import collections,gzip,hashlib,json,re,sys
folder=Path(sys.argv[1]) if sys.argv[1:] else Path(__file__).resolve().parent
sha=lambda raw:hashlib.sha256(raw).hexdigest()
def unpack(name):return gzip.decompress((folder/(name+'.gz')).read_bytes())
original=json.loads(unpack('command.json'))
manifest=json.loads((folder/'archive-manifest.json').read_text())
for name, hashes in manifest.items():
 assert sha((folder/name).read_bytes())==hashes['archive_sha256']
 assert sha(gzip.decompress((folder/name).read_bytes()))==hashes['raw_sha256']
assert original['complete'] and original['exit_code']==0
assert original['inputs_unchanged'] and original['head_unchanged']
raw=unpack('run.log')
assert original['command']==['cargo','test','--workspace','--locked','--','--test-threads=2']
metadata=json.loads(unpack('cargo-metadata.json'));workspace=Path(metadata['workspace_root'])
expected=[]
for package in metadata['packages']:
 if package['id'] not in metadata['workspace_members']:continue
 for target in package['targets']:
  library=any(kind in ['lib','rlib','dylib','cdylib','staticlib','proc-macro'] for kind in target['kind'])
  if target['test'] and (library or any(kind in ['bin','test'] for kind in target['kind'])):
   source=str(Path(target['src_path']).relative_to(workspace))
   expected.append((package['name'],target['name'],False,source))
  if target['doctest'] and library:expected.append((package['name'],target['name'],True,None))
assert len(expected)==len(set(expected)), 'duplicate expected target'
fingerprints=json.loads(unpack('cargo-fingerprints.json'));by_suffix=collections.defaultdict(list)
for path,datum in fingerprints.items():
 assert sha(datum['content'].encode())==datum['sha256'],path
 json.loads(datum['content'])
 directory=Path(path).parent.name;package,separator,suffix=directory.rpartition('-')
 assert separator and re.fullmatch('[0-9a-f]+',suffix),directory
 assert Path(path).name.startswith('test-') and path.endswith('.json')
 by_suffix[suffix].append((package,path))
text=raw.decode(errors='strict')
markers=list(re.finditer(r'^\s*(?:Running (.+?) \(([^\n]+)\)|Doc-tests ([^\n]+))\s*$',text,re.M))
observed=[];results=[]
for index,marker in enumerate(markers):
 body=text[marker.end():markers[index+1].start() if index+1<len(markers) else len(text)]
 summaries=re.findall(r'^test result: (ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured; (\d+) filtered out;',body,re.M)
 assert len(summaries)==1,(marker.groups(),summaries)
 status,passed,failed,ignored,measured,filtered=summaries[0]
 assert (status,failed,filtered)==('ok','0','0'),(marker.groups(),summaries)
 results.append((int(passed),int(ignored)))
 if marker.group(3):
  name=marker.group(3);identity=(name.replace('_','-'),name,True,None)
 else:
  binary=Path(marker.group(2)).name;name,separator,suffix=binary.rpartition('-')
  assert separator and re.fullmatch('[0-9a-f]+',suffix),binary
  hits=by_suffix[suffix];assert len(hits)==1,(binary,hits)
  package,path=hits[0]
  source='crates/'+package+'/'+marker.group(1).removeprefix('unittests ')
  hits=[target for target in expected if target[0]==package and not target[2] and target[1].replace('-','_')==name and target[3]==source]
  assert len(hits)==1,(binary,package,source,hits)
  identity=hits[0]
 observed.append(identity)
assert collections.Counter(observed)==collections.Counter(expected),'missing, duplicated or unexpected target'
report={'source_head':original['head'],'expected_count':len(expected),'observed_count':len(observed),'passed_tests':sum(x[0] for x in results),'ignored_tests':sum(x[1] for x in results),'all_native_and_doc_targets_pass':True,'cargo_exit_code':original['exit_code'],'qualification':__doc__.strip(),'inputs_sha256':{name:sha((folder/name).read_bytes()) for name in list(manifest)}}
print(json.dumps(report,indent=2))
