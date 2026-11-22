from pathlib import Path
import hashlib,json,os,subprocess,time
ROOT=Path(__file__).resolve().parents[2];OUT=Path(__file__).resolve().parent
ENV=os.environ|{'XDG_RUNTIME_DIR':'/run/user/1000','CARGO_BUILD_JOBS':'2','RUSTUP_TOOLCHAIN':'stable','CARGO_TARGET_DIR':str(ROOT.parent/'m12-helper-target')}
paths=subprocess.check_output(['git','ls-files','--cached','--others','--exclude-standard','-z'],cwd=ROOT).decode().split('\0')
inputs={p:hashlib.sha256((ROOT/p).read_bytes()).hexdigest() for p in paths if p and not p.startswith('results/') and (ROOT/p).is_file()}
record={'head':subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),'inputs':inputs,'commands':[],'complete':False}
def save():(OUT/'final-gates.json').write_text(json.dumps(record,indent=2)+'\n')
def run(label,args,scope=True):
 start=time.time()
 with (OUT/(label+'.log')).open('w') as out:
  p=subprocess.run((['systemd-run','--user','--scope','-p','MemoryMax=4G','--'] if scope else [])+args,cwd=ROOT,env=ENV,stdout=out,stderr=subprocess.STDOUT)
 record['commands'].append({'label':label,'command':args,'exit_code':p.returncode,'seconds':time.time()-start});save();print(record['commands'][-1],flush=True)
save()
# run-suite records the expected initial baseline mismatch in its own receipt.
run('final-suite-driver',['python3','results/m12-constant-definitions/run-suite.py','final'],False)
run('declarations',['cargo','test','-p','suss-compile','-p','suss-cli','--locked',
 '--test','portable_resolution','--test','portable_definitions',
 '--test','compiled_macro_definitions','--test','compiled_macro_declaration_metadata',
 '--test','compiled_macro_declaration_functions','--test','compiled_macro_repeated_declarations',
 '--test','compiled_aot_direct_linking','--test','portable_forward_declarations',
 '--','--test-threads=2'])
run('bootstrap',['sh','scripts/verify-bootstrap.sh'])
run('core-manifest',['python3','scripts/core_import.py','--check'])
run('bootstrap-source',['python3','scripts/apply_bootstrap.py','--check'])
record.update(complete=True,inputs_unchanged=all(hashlib.sha256((ROOT/f).read_bytes()).hexdigest()==v for f,v in inputs.items()));save();print('final gates terminal',record['inputs_unchanged'],flush=True)
