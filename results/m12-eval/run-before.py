"""Run the real eval regressions against immutable pre-eval compiler libraries."""
from pathlib import Path
import hashlib,json,os,signal,subprocess,sys,time
signal.signal(signal.SIGHUP,signal.SIG_IGN)
ROOT=Path(__file__).resolve().parents[2];BASE=Path(__file__).resolve().parent
scope=os.environ['SUSS_EVAL_SCOPE'];assert scope in Path('/proc/self/cgroup').read_text()
OUT=BASE/'before-compiled-eval';OUT.mkdir(exist_ok=True);assert not (OUT/'command.json').exists()
TARGET=ROOT.parent/'m12-review-repair-target';DEPS=TARGET/'debug/deps';FP=TARGET/'debug/.fingerprint'
sha=lambda raw:hashlib.sha256(raw).hexdigest()
cli_path=max(FP.glob('suss-cli-*/bin-suss.json'),key=lambda p:p.stat().st_mtime_ns)
cli=json.loads(cli_path.read_text())
def exact(parent,name):
 wanted=next(row[3] for row in parent['deps'] if row[1]==name);matches=[]
 for path in FP.glob('*/lib-'+name):
  try:fingerprint=int.from_bytes(bytes.fromhex(path.read_text()),'little')
  except ValueError:continue
  library=DEPS/('lib'+name+'-'+path.parent.name.rsplit('-',1)[1]+'.rlib')
  if fingerprint==wanted and library.is_file():matches.append((library,json.loads(path.with_name(path.name+'.json').read_text())))
 assert len(matches)==1,(name,matches)
 return matches[0]
library,compiler=exact(cli,'suss_compile')
libs={'suss_compile':library,**{name:exact(compiler,name)[0] for name in ['suss_reader','serde_json','wasmtime','wasm_encoder','wasmparser']}}
artifact=ROOT.parent/'m12-eval-before-artifact';artifact.mkdir(exist_ok=True)
scratch=artifact/'temporary';scratch.mkdir(exist_ok=True);exe=artifact/'clojure-eval';assert not exe.exists()
source=ROOT/'crates/suss-cli/tests/clojure_eval.rs'
command=['rustc','--edition=2024','--test',str(source),'-L','dependency='+str(DEPS),'-o',str(exe)]
for name,path in libs.items():command.extend(['--extern',name+'='+str(path)])
paths=subprocess.check_output(['git','ls-files','--cached','--others','--exclude-standard','-z'],cwd=ROOT).decode().split('\0')
inputs={p:sha((ROOT/p).read_bytes()) for p in paths if p and not p.startswith('results/') and (ROOT/p).is_file()}
for path in (BASE/'primary').iterdir():
 if path.is_file():inputs[str(path.relative_to(ROOT))]=sha(path.read_bytes())
record={'head':subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),'scope':scope,'recorder_pid':os.getpid(),'runner_sha256':sha(Path(__file__).read_bytes()),'inputs':inputs,'libraries':{str(path):sha(path.read_bytes()) for path in libs.values()},'commands':[],'complete':False,'qualification':'Executing unchanged public-API regressions against the immutable pre-eval compiler selected from the frozen PR507 native target fingerprints. No Cargo target is written, no eval implementation is injected, and this is not a Cargo or post-implementation pass.'}
def save():(OUT/'command.json').write_text(json.dumps(record,indent=2)+'\n')
env=os.environ|{'CARGO_MANIFEST_DIR':str(ROOT/'crates/suss-cli'),'RUSTUP_TOOLCHAIN':'stable','TMPDIR':str(scratch)}
save()
for name,cmd in [('build',command),('native-regressions',[str(exe),'compiled_eval_','--nocapture','--test-threads=2'])]:
 start=time.time()
 with (OUT/(name+'.stdout')).open('w') as stdout,(OUT/(name+'.stderr')).open('w') as stderr:
  result=subprocess.run(cmd,cwd=ROOT,env=env,stdout=stdout,stderr=stderr)
 record['commands'].append({'label':name,'command':cmd,'exit_code':result.returncode,'seconds':time.time()-start});save();print(record['commands'][-1],flush=True)
 if result.returncode:break
record.update(complete=True,inputs_unchanged=all(sha((ROOT/p).read_bytes())==h for p,h in inputs.items()),libraries_unchanged=all(sha(Path(p).read_bytes())==h for p,h in record['libraries'].items()))
save();print({k:v for k,v in record.items() if k not in ['inputs','libraries','commands']},flush=True)
raise SystemExit(record['commands'][-1]['exit_code'])
