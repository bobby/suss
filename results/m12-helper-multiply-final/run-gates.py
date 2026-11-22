"""Validate final source; run wrapper profiling alongside independent CLI checks."""
from pathlib import Path
import hashlib,json,os,subprocess,time
ROOT=Path(__file__).resolve().parents[2];OUT=Path(__file__).resolve().parent
ENV=os.environ|{'CARGO_TARGET_DIR':str(ROOT.parent/'m12-helper-target'),'CARGO_BUILD_JOBS':'2','RUSTUP_TOOLCHAIN':'stable','XDG_RUNTIME_DIR':'/run/user/1000'}
inputs=json.loads((OUT/'inputs.json').read_text());record={'base':'143062ce','inputs':inputs,'commands':[],'complete':False}
def save():(OUT/'commands.json').write_text(json.dumps(record,indent=2)+'\n')
def cargo(package,*targets):
 command=['cargo','test','-p',package,'--locked']
 for target in targets:command+=['--test',target]
 return command+['--','--test-threads=2']
def run(label,command):
 assert all(hashlib.sha256((ROOT/p).read_bytes()).hexdigest()==h for p,h in inputs.items()),'source changed'
 started=time.time()
 with (OUT/(label+'.log')).open('w') as log:r=subprocess.run(['systemd-run','--user','--scope','-p','MemoryMax=4G','--',*command],cwd=ROOT,env=ENV,stdout=log,stderr=subprocess.STDOUT)
 record['commands'].append({'label':label,'command':command,'exit_code':r.returncode,'seconds':time.time()-started});save();print(label,r.returncode,flush=True)
 if r.returncode:raise SystemExit(r.returncode)
save()
run('pinned-rebuild',['env','-u','RUSTUP_TOOLCHAIN','python3','scripts/numeric_runtime.py','--rebuild-check'])
run('sort-final',['cargo','test','-p','suss-compile','--locked','--lib','binding_sort_tests','--','--test-threads=2'])
run('compiler',cargo('suss-compile','bigint_helper','portable_comparator_foundations','runtime_abi','portable_calling_convention'))
run('profile-build',['cargo','build','-p','suss-compile','--locked','--profile','test','--example','bigint_runtime'])
profile_log=(OUT/'profile-driver.log').open('w')
profile=subprocess.Popen(['python3',str(OUT/'profile.py')],cwd=ROOT,env=ENV,stdout=profile_log,stderr=subprocess.STDOUT)
try:
 run('cli',cargo('suss-cli','compiled_record_default_budget','clojure_equality_bitwise','clojure_ratios','clojure_bigint_integers','clojure_bigints','compiled_macro_compiler_catalog'))
 run('bootstrap',['sh','scripts/verify-bootstrap.sh'])
 run('core-import',['python3','scripts/core_import.py','--check'])
finally:
 profile_code=profile.wait();profile_log.close();record['profile_exit_code']=profile_code;save();print('profile',profile_code,flush=True)
assert profile_code==0
assert all(hashlib.sha256((ROOT/p).read_bytes()).hexdigest()==h for p,h in inputs.items())
record.update(complete=True,inputs_unchanged=True);save()
