from pathlib import Path
import gzip,hashlib,json,re,subprocess,sys
here=Path(__file__).resolve().parent;root=here.parents[2]
sha=lambda data:hashlib.sha256(data).hexdigest()
summary=json.loads((here/'summary.json').read_text());cache={}
for name,digest in summary['archive_hashes'].items():assert sha((here/name).read_bytes())==digest,name
for phase,commands in summary['phases'].items():
 state=json.loads(gzip.decompress((here/phase/'commands.json.gz').read_bytes()))
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
print('Verified',len(summary['phases']),'exact historical source overlays, runners, command terminals and raw log totals; all recorded failures remain failures.')

if '--current' in sys.argv:
 publication=json.loads((here/'publication.json').read_text())
 state=json.loads(gzip.decompress((here/publication['phase']/'commands.json.gz').read_bytes()))
 expected={n:h for n,h in state['inputs'].items() if not n.startswith('results/')}
 current=set(subprocess.check_output(['git','ls-files','-co','--exclude-standard'],cwd=root,text=True).splitlines())
 current={n for n in current if not n.startswith('results/') and (root/n).is_file()}
 assert current==set(expected),(sorted(current-set(expected)),sorted(set(expected)-current))
 overrides=publication['documentation_sha256']
 assert set(overrides)=={'docs/roadmap/handoff.md','docs/compatibility/README.md','docs/runtime/native-var-values.md','results/m12-eval/README.md'}
 for n,h in expected.items(): assert sha((root/n).read_bytes())==overrides.get(n,h),n
 for n,h in overrides.items(): assert sha((root/n).read_bytes())==h,n
 print('Current nonresult source matches the final executed overlay plus explicit documentation publication.')
