from pathlib import Path
import hashlib,json,os,subprocess,time,sys
ROOT=Path(__file__).resolve().parents[2];OUT=Path(__file__).resolve().parent
if len(sys.argv)>1: OUT=OUT/sys.argv[1]
OUT.mkdir(parents=True,exist_ok=True)
ENV=os.environ|{'XDG_RUNTIME_DIR':'/run/user/1000','CARGO_BUILD_JOBS':'2','RUSTUP_TOOLCHAIN':'stable','CARGO_TARGET_DIR':str(ROOT.parent/'m12-helper-target'),'SUSS_CLOJURE_TEST_SUITE_CAPTURE':str(OUT/'suite-capture')}
ENV.pop('SUSS_CLOJURE_TEST_SUITE_WRITE',None)
paths=subprocess.check_output(['git','ls-files','--cached','--others','--exclude-standard','-z'],cwd=ROOT).decode().split('\0')
inputs={p:hashlib.sha256((ROOT/p).read_bytes()).hexdigest() for p in paths if p and not p.startswith('results/') and (ROOT/p).is_file()}
record={'head':subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),'inputs':inputs,'commands':[],'complete':False}
def save():(OUT/'suite-commands.json').write_text(json.dumps(record,indent=2)+'\n')
def run(label,args):
 start=time.time()
 with (OUT/(label+'.log')).open('w') as out:
  p=subprocess.run(['systemd-run','--user','--scope','-p','MemoryMax=4G','--',*args],cwd=ROOT,env=ENV,stdout=out,stderr=subprocess.STDOUT)
 record['commands'].append({'label':label,'command':args,'exit_code':p.returncode,'seconds':time.time()-start});save();print(record['commands'][-1],flush=True)
 return p.returncode
save()
if run('suite-build',['cargo','test','-p','suss-compile','--locked','--test','clojure_test_suite','--no-run','--message-format=json'])==0:
 rows=[]
 for line in (OUT/'suite-build.log').read_text().splitlines():
  try: rows.append(json.loads(line))
  except ValueError: pass
 exe=next(row['executable'] for row in rows if row.get('reason')=='compiler-artifact' and row.get('target',{}).get('name')=='clojure_test_suite' and row.get('executable'))
 record['executable_sha256']=hashlib.sha256(Path(exe).read_bytes()).hexdigest();save()
 run('suite-run',[exe,'clojure_test_suite_matches_reviewed_baseline','--exact','--nocapture','--test-threads=2'])
 run('suite-lock',['python3','scripts/clojure_test_suite.py'])
record.update(complete=True,inputs_unchanged=all(hashlib.sha256((ROOT/f).read_bytes()).hexdigest()==v for f,v in inputs.items()));save();print('suite receipt complete',record['inputs_unchanged'],flush=True)
