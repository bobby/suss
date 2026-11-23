from pathlib import Path
import hashlib,json,os,subprocess,time
ROOT=Path(__file__).resolve().parents[2];OUT=Path(__file__).resolve().parent
FIXTURES=OUT
if len(__import__("sys").argv)>1: OUT=OUT/__import__("sys").argv[1]
OUT.mkdir(parents=True,exist_ok=True)
env=os.environ|{'XDG_RUNTIME_DIR':'/run/user/1000','CARGO_BUILD_JOBS':'2','RUSTUP_TOOLCHAIN':'stable','CARGO_TARGET_DIR':str(ROOT.parent/'m12-decimal-target')}
commands=[('decimal-and-abi', ['cargo', 'test', '--workspace', '--locked', '--no-fail-fast', '--test', 'portable_value_decoder', '--test', 'portable_calling_convention', '--test', 'portable_comparator_foundations', '--test', 'decimal_values', '--test', 'clojure_decimal_values', '--test', 'clojure_decimal_arithmetic', '--', '--test-threads=2']), ('numeric-inference', ['cargo', 'test', '--workspace', '--locked', '--no-fail-fast', '--test', 'clojure_division_overloads', '--test', 'clojure_unboxed_locals', '--test', 'clojure_raw_numeric_core', '--test', 'clojure_primitive_method_types', '--test', 'clojure_numeric_captures', '--test', 'clojure_local_numeric_hints', '--test', 'clojure_numeric_box_reuse', '--test', 'clojure_exact_inline', '--test', 'clojure_unchecked_ratios', '--test', 'numeric_operand_types', '--test', 'portable_i31_operations', '--', '--test-threads=2']), ('workspace-compile', ['cargo', 'check', '--workspace', '--tests', '--locked']), ('core-manifest', ['python3', 'scripts/core_import.py', '--check']), ('bootstrap-source', ['python3', 'scripts/apply_bootstrap.py', '--check'])]

paths=subprocess.check_output(['git','ls-files','--cached','--others','--exclude-standard','-z'],cwd=ROOT).decode().split('\0')
inputs={p:hashlib.sha256((ROOT/p).read_bytes()).hexdigest() for p in paths if p and not p.startswith('results/') and (ROOT/p).is_file()}
record={'fixtures':{str(p.relative_to(ROOT)):hashlib.sha256(p.read_bytes()).hexdigest() for p in FIXTURES.glob('*cases.json')},'harness_sha256':hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),'head':subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),'submodule_head':subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT/'clojurescript',text=True).strip(),'inputs':inputs,'requested_commands':commands,'commands':[],'complete':False}
assert not (OUT/'gates.json').exists()
def save():(OUT/'gates.json').write_text(json.dumps(record,indent=2)+'\n')
save()
for label,command in commands:
 start=time.time()
 with (OUT/(label+'.log')).open('w') as out:
  result=subprocess.run(['systemd-run','--user','--scope','-p','MemoryMax=4G','--',*command],cwd=ROOT,env=env,stdout=out,stderr=subprocess.STDOUT)
 row={'label':label,'command':command,'exit_code':result.returncode,'seconds':time.time()-start};record['commands'].append(row);save();print(row,flush=True)
 if result.returncode:break
record.update(complete=True,all_passed=len(record['commands'])==len(commands) and all(r['exit_code']==0 for r in record['commands']),inputs_unchanged=all(hashlib.sha256((ROOT/p).read_bytes()).hexdigest()==v for p,v in inputs.items()));save();print({k:v for k,v in record.items() if k not in ['inputs','requested_commands','commands']},flush=True)
