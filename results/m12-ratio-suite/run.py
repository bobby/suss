"""Compile a frozen suite probe, then execute it without holding Cargo's lock."""
from pathlib import Path
import hashlib,json,os,subprocess,time
ROOT=Path(__file__).resolve().parents[2];OUT=Path(__file__).resolve().parent
ENV=os.environ|{'CARGO_BUILD_JOBS':'2','RUSTUP_TOOLCHAIN':'stable','XDG_RUNTIME_DIR':'/run/user/1000','CARGO_TARGET_DIR':str(ROOT.parents[2]/'target/probe/m12-ratio-target'),'SUSS_CLOJURE_TEST_SUITE_CAPTURE':str(OUT)}
record={'head':subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),'inputs':json.loads((ROOT/'results/m12-ratio-values/commands.json').read_text())['inputs'],'commands':[],'complete':False}
def save():(OUT/'command.json').write_text(json.dumps(record,indent=2)+'\n')
save()
command=['cargo','test','-p','suss-compile','--locked','--test','clojure_test_suite','--no-run','--message-format=json']
started=time.time();r=subprocess.run(['systemd-run','--user','--scope','-p','MemoryMax=4G','--',*command],cwd=ROOT,env=ENV,capture_output=True,text=True)
(OUT/'build.log').write_text(r.stdout+r.stderr);record['commands'].append({'command':command,'exit_code':r.returncode,'seconds':time.time()-started});save();assert r.returncode==0,r.stderr
artifacts=[]
for line in r.stdout.splitlines():
 try:row=json.loads(line)
 except json.JSONDecodeError:continue
 if row.get('reason')=='compiler-artifact' and row.get('target',{}).get('name')=='clojure_test_suite' and row.get('executable'):artifacts.append(row['executable'])
assert len(artifacts)==1,artifacts
binary=Path(artifacts[0]);record['executable_sha256']=hashlib.sha256(binary.read_bytes()).hexdigest()
command=[str(binary),'clojure_test_suite_matches_reviewed_baseline','--exact','--nocapture','--test-threads=2'];save()
started=time.time()
with (OUT/'run.log').open('w') as log:r=subprocess.run(['systemd-run','--user','--scope','-p','MemoryMax=4G','--',*command],cwd=ROOT,env=ENV,stdout=log,stderr=subprocess.STDOUT)
record['commands'].append({'command':command,'exit_code':r.returncode,'seconds':time.time()-started});record['complete']=True
assert all(hashlib.sha256((ROOT/p).read_bytes()).hexdigest()==digest for p,digest in record['inputs'].items()),'Inputs changed during the suite capture.'
save();print(json.dumps(record['commands']),flush=True)
