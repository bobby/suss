from pathlib import Path
import gzip,hashlib,json,re,subprocess,sys
here=Path(__file__).resolve().parent;root=here.parents[2]
sha=lambda data:hashlib.sha256(data).hexdigest()
summary=json.loads((here/'summary.json').read_text());cache={}
for name,digest in summary['archive_hashes'].items():assert sha((here/name).read_bytes())==digest,name
states={}
for phase,commands in summary['phases'].items():
 state=json.loads(gzip.decompress((here/phase/'commands.json.gz').read_bytes()))
 states[phase]=state
 assert state['complete'] and state['inputs_unchanged'] and state['binaryen_unchanged'],phase
 assert sha(gzip.decompress((here/phase/'runner.py.gz').read_bytes()))==state['runner_sha256'],phase
 assert {row['name']:row['exit_code'] for row in state['commands']}=={n:r['exit_code'] for n,r in commands.items()}
 for row in state['commands']:
  assert row['complete']
  data=gzip.decompress((here/phase/(row['name']+'.log.gz')).read_bytes())
  actual=[list(map(int,m)) for m in re.findall(r'test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed;',data.decode(errors='replace'))]
  assert actual==commands[row['name']]['rust_totals'],(phase,row['name'])
 for name,digest in state['inputs'].items():
  overlay=here/phase/'overlay'/(name+'.gz');shared=here/'inputs'/(digest+'.gz')
  if overlay.exists() or shared.exists():data=gzip.decompress((overlay if overlay.exists() else shared).read_bytes())
  else:
   key=(state['head'],name)
   if key not in cache:
    current=root/name;value=current.read_bytes() if current.is_file() else b''
    cache[key]=value if sha(value)==digest else subprocess.check_output(['git','show',state['head']+':'+name],cwd=root)
   data=cache[key]
  assert sha(data)==digest,(phase,name)

assert len(states)==15
assert all(row['exit_code']==0 for row in states['guarded-murmur-profile-working']['commands'])
ordinary=states['ordinary-unbound-working']
assert {row['name']:row['exit_code'] for row in ordinary['commands'] if row['exit_code']}=={'complete-public-eval':101}
log=gzip.decompress((here/'ordinary-unbound-working/complete-public-eval.log.gz').read_bytes()).decode()
assert 'test result: FAILED. 87 passed; 1 failed;' in log
assert 'Unresolved Runtime name when-let' in log and 'Unresolved Macro name when-let' in log
assert log.count('namespace-boundary-primary-match ')==10
assert log.count('isolated-source-primary-match ')==40
publication=states['abi13-publication']
assert all(row['exit_code']==0 for row in publication['commands'])
assert sum(sum(t[0] for t in row['rust_totals']) for row in summary['phases']['abi13-publication'].values())==67
before={n:h for n,h in ordinary['inputs'].items() if not n.startswith('results/')}
after={n:h for n,h in publication['inputs'].items() if not n.startswith('results/')}
assert before.keys()==after.keys()
assert {n for n in before if before[n]!=after[n]}=={'crates/suss-compile/src/runtime_abi.rs'}
abi='crates/suss-compile/src/runtime_abi.rs'
old=gzip.decompress((here/'inputs'/(ordinary['inputs'][abi]+'.gz')).read_bytes())
new=gzip.decompress((here/'inputs'/(publication['inputs'][abi]+'.gz')).read_bytes())
assert old.count(b'pub const VERSION: u32 = 12;')==1
assert old.replace(b'pub const VERSION: u32 = 12;',b'pub const VERSION: u32 = 13;')==new
for name,delta in summary['publication_prose_delta'].items():
 assert delta['validated']==publication['inputs'].get(name)
 assert sha(gzip.decompress((here/delta['archive']).read_bytes()))==delta['published']
if '--current' in sys.argv:
 for name,digest in after.items():
  expected=summary['publication_prose_delta'].get(name,{}).get('published',digest)
  assert sha((root/name).read_bytes())==expected,name
 for name,delta in summary['publication_prose_delta'].items():assert sha((root/name).read_bytes())==delta['published'],name
print('Verified15 exact source/runner/terminal/log histories, unchanged primary inputs, ABI13-only source delta,67 ABI13 Rust executions and explicit prose deltas. The historical complete eval target is87/88 with the core macro Var failure retained; no full issue or promotion claim.')
