from pathlib import Path
import hashlib,json,os,subprocess,time
ROOT=Path(__file__).resolve().parents[2]; OUT=Path(__file__).resolve().parent/'gates'
OUT.mkdir(exist_ok=True); assert not (OUT/'commands.json').exists()
env=os.environ|{'XDG_RUNTIME_DIR':'/run/user/1000','CARGO_BUILD_JOBS':'2','RUSTUP_TOOLCHAIN':'stable','CARGO_TARGET_DIR':str(ROOT.parent/'m12-decimal-followup-target')}
commands=[('unboxed-locals', ['cargo', 'test', '--locked', '-p', 'suss-compile', '--test', 'portable_unboxed_locals', '--', '--test-threads=2']), ('numeric-format', ['cargo', 'test', '--locked', '-p', 'suss-compile', '--test', 'runtime_abi', 'runtime_abi_numeric', '--', '--test-threads=2']), ('decimal-regressions', ['cargo', 'test', '--workspace', '--locked', '--no-fail-fast', '--test', 'clojure_decimal_constructors', '--test', 'clojure_decimal_arithmetic', '--test', 'clojure_decimal_conversions', '--test', 'math_context_constructors', '--test', 'decimal_values', '--test', 'portable_calling_convention', '--test', 'portable_value_decoder', '--test', 'clojure_unboxed_locals', '--', '--test-threads=2']), ('aot-size', ['cargo', 'test', '--locked', '-p', 'suss-cli', '--test', 'runtime_namespaces', 'aot_namespace_frame_size_is_bounded_per_form', '--', '--exact', '--nocapture', '--test-threads=2']), ('bootstrap', ['sh', 'scripts/verify-bootstrap.sh']), ('foundation', ['python3', 'results/m12-decimal-provenance/run-foundation.py', 'after-raw-strings']), ('workspace-compile', ['cargo', 'check', '--workspace', '--tests', '--locked'])]
paths=subprocess.check_output(['git','ls-files','--cached','--others','--exclude-standard','-z'],cwd=ROOT).decode().split('\0')
inputs={p:hashlib.sha256((ROOT/p).read_bytes()).hexdigest() for p in paths if p and not p.startswith('results/') and (ROOT/p).is_file()}
record={'head':subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),'inputs':inputs,'fixtures':{str(p.relative_to(ROOT)):hashlib.sha256(p.read_bytes()).hexdigest() for d in ['m12-decimal-arithmetic','m12-decimal-integration','m12-decimal-conversions','m12-context-constructors'] for p in (ROOT/'results'/d).glob('*cases.json')} | {'results/m12-context-constructors/bmp-digits.json':hashlib.sha256((ROOT/'results/m12-context-constructors/bmp-digits.json').read_bytes()).hexdigest()},'requested_commands':commands,'commands':[],'complete':False}
def save():(OUT/'commands.json').write_text(json.dumps(record,indent=2)+'\n')
save()
for label,command in commands:
 t=time.time()
 with (OUT/(label+'.log')).open('w') as log:r=subprocess.run(['systemd-run','--user','--scope','-p','MemoryMax=4G','--',*command],cwd=ROOT,env=env,stdout=log,stderr=subprocess.STDOUT)
 row={'label':label,'command':command,'exit_code':r.returncode,'seconds':time.time()-t};record['commands'].append(row);save();print(row,flush=True)
 if r.returncode:break
record.update(complete=True,inputs_unchanged=all(hashlib.sha256((ROOT/p).read_bytes()).hexdigest()==h for p,h in inputs.items()));save()
