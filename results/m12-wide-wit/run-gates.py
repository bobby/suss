"""Retain terminal adapter/ABI checks against frozen implementation inputs."""
from pathlib import Path
import hashlib,json,os,subprocess,time
ROOT=Path(__file__).resolve().parents[2];OUT=Path(__file__).resolve().parent
ENV=os.environ|{'CARGO_BUILD_JOBS':'2','RUSTUP_TOOLCHAIN':'stable','XDG_RUNTIME_DIR':'/run/user/1000','CARGO_TARGET_DIR':str(ROOT.parent/'m12-wide-wit-target')}
def cargo(package,*targets):
 command=['cargo','test','-p',package,'--locked']
 for target in targets:command+=['--test',target]
 return command+['--','--test-threads=2']
commands=[
 ('wit-numbers',cargo('suss-compile','portable_aot_wit_numbers')),
 ('adapter-controls',cargo('suss-compile','portable_aot','portable_aot_async','portable_aot_lists','portable_aot_options','portable_aot_strings')),
 ('abi-controls',cargo('suss-compile','runtime_abi','portable_calling_convention')),
 ('param-hint-controls',cargo('suss-cli','compiled_export_param_hints')),
 ('bootstrap',['sh','scripts/verify-bootstrap.sh']),
 ('core-import',['python3','scripts/core_import.py','--check']),
 ('numeric-helper',['python3','scripts/numeric_runtime.py','--check']),
]
paths=subprocess.check_output(['git','ls-files','-z'],cwd=ROOT).decode().split('\0')
inputs={p:hashlib.sha256((ROOT/p).read_bytes()).hexdigest() for p in paths if p and not p.startswith(('results/','docs/')) and (ROOT/p).is_file()}
record={'base':subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),'inputs':inputs,'commands':[],'complete':False,'environment':{k:ENV[k] for k in ['CARGO_BUILD_JOBS','RUSTUP_TOOLCHAIN','CARGO_TARGET_DIR']}}
def save():(OUT/'commands.json').write_text(json.dumps(record,indent=2)+'\n')
save()
for label,command in commands:
 start=time.time()
 with (OUT/(label+'.log')).open('w') as log:r=subprocess.run(['systemd-run','--user','--scope','-p','MemoryMax=4G','--',*command],cwd=ROOT,env=ENV,stdout=log,stderr=subprocess.STDOUT)
 record['commands'].append({'label':label,'command':command,'exit_code':r.returncode,'seconds':time.time()-start});save();print(label,r.returncode,flush=True)
 if r.returncode:raise SystemExit(r.returncode)
assert all(hashlib.sha256((ROOT/p).read_bytes()).hexdigest()==digest for p,digest in inputs.items()),'Implementation inputs changed during gates.'
record.update(complete=True,inputs_unchanged=True);save()
