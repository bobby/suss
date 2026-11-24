"""Corrected target audit; preserves the original observer and lost-exit qualification."""
import gzip, hashlib, json, pathlib, re, subprocess, sys
root=pathlib.Path(sys.argv[1]); folder=pathlib.Path(sys.argv[2])
prior=json.loads((folder/'observation.json').read_text())
assert prior['complete']
raw=gzip.decompress((folder/'run.log.gz').read_bytes())
assert hashlib.sha256(raw).hexdigest()==prior['archives']['run.log']['raw_sha256']
receipt_raw=gzip.decompress((folder/'original-command.json.gz').read_bytes())
assert hashlib.sha256(receipt_raw).hexdigest()==prior['original_receipt_sha256']
receipt=json.loads(receipt_raw)
meta=json.loads(subprocess.check_output(['cargo','metadata','--locked','--no-deps','--format-version','1'],cwd=root))
expected=[]
for p in meta['packages']:
 if p['id'] not in meta['workspace_members']:continue
 for t in p['targets']:
  library=any(k in ['lib','rlib','dylib','cdylib','staticlib','proc-macro'] for k in t['kind'])
  if t['test'] and (library or any(k in ['bin','test'] for k in t['kind'])):
   expected.append((p['name'],t['name'],False,str(pathlib.Path(t['src_path']).relative_to(root))))
  if t['doctest'] and library:expected.append((p['name'],t['name'],True,None))
text=raw.decode(errors='strict'); markers=list(re.finditer(r'^\s*(?:Running (.+?) \(([^\n]+)\)|Doc-tests ([^\n]+))\s*$',text,re.M))
observed=[]
for i,m in enumerate(markers):
 body=text[m.end():markers[i+1].start() if i+1<len(markers) else len(text)]
 summaries=re.findall(r'^test result: (ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured; (\d+) filtered out;',body,re.M)
 if m.group(3):
  package=m.group(3).replace('_','-'); name=m.group(3); source=None; fingerprint=None
 else:
  binary=pathlib.Path(m.group(2));name,sep,suffix=binary.name.rpartition('-');assert sep and re.fullmatch('[0-9a-f]+',suffix)
  fps=list((binary.parent.parent/'.fingerprint').glob('*-'+suffix)); assert len(fps)==1,(binary,fps)
  fingerprint=str(fps[0]);package=fps[0].name[:-(len(suffix)+1)]
  assert list(fps[0].glob('test-*.json')),(binary,fps)
  label=m.group(1);source='crates/'+package+'/'+(label.removeprefix('unittests '))
  names=[e[1] for e in expected if e[0]==package and not e[2] and e[1].replace('-','_')==name and e[3]==source]
  assert len(names)==1,(package,name,source,names);name=names[0]
 observed.append({'package':package,'name':name,'doc':bool(m.group(3)),'source':source,'fingerprint':fingerprint,'summaries':summaries})
missing=[]
for p,n,d,s in expected:
 hits=[x for x in observed if (x['package'],x['name'],x['doc'],x['source'])==(p,n,d,s)]
 if len(hits)!=1:missing.append({'target':[p,n,d,s],'matches':len(hits)})
unexpected=[x for x in observed if (x['package'],x['name'],x['doc'],x['source']) not in expected]
changed=[p for p,h in receipt['inputs'].items() if not (root/p).is_file() or hashlib.sha256((root/p).read_bytes()).hexdigest()!=h]
passed=not missing and not unexpected and all(len(x['summaries'])==1 and x['summaries'][0][0]=='ok' and x['summaries'][0][2]=='0' and x['summaries'][0][5]=='0' for x in observed)
out={'head':receipt['head'],'head_unchanged':subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip()==receipt['head'],'inputs_unchanged':not changed,'changed_inputs':changed,'cargo_exit_code':None,'qualification':prior['qualification'],'corrections':['Include explicit rlib Cargo library kind in native and doctest inventory.','Disambiguate same-named integration targets using Cargo binary fingerprint package identities.'],'original_observation_sha256':hashlib.sha256((folder/'observation.json').read_bytes()).hexdigest(),'auditor_sha256':hashlib.sha256(pathlib.Path(__file__).read_bytes()).hexdigest(),'expected_count':len(expected),'observed_count':len(observed),'missing':missing,'unexpected':unexpected,'all_native_and_doc_targets_pass':passed,'passed_tests':sum(int(x['summaries'][0][1]) for x in observed),'ignored_tests':sum(int(x['summaries'][0][3]) for x in observed),'observed':observed}
path=folder/'corrected-target-audit.json';assert not path.exists();path.write_text(json.dumps(out,indent=2)+'\n')
print({k:v for k,v in out.items() if k!='observed'})
assert passed and out['head_unchanged'] and out['inputs_unchanged']
