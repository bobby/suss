"""Freeze the integrated Ratio implementation and retain terminal scoped gates."""
from pathlib import Path
import hashlib,json,os,subprocess,time
ROOT=Path(__file__).resolve().parents[2];OUT=Path(__file__).resolve().parent
BASE='a5c59e83'
assert subprocess.run(['git','merge-base','--is-ancestor',BASE,'HEAD'],cwd=ROOT).returncode==0,'Integrate published observation/source fixes before gating.'
ENV=os.environ|{'CARGO_BUILD_JOBS':'2','RUSTUP_TOOLCHAIN':'stable','XDG_RUNTIME_DIR':'/run/user/1000','CARGO_TARGET_DIR':str(ROOT.parents[2]/'target/probe/m12-ratio-target')}
def cargo(package,*targets):
 command=['cargo','test','-p',package,'--locked']
 for target in targets:
  assert (ROOT/'crates'/package/'tests'/(target+'.rs')).is_file(),target
  command+=['--test',target]
 return command+['--','--test-threads=2']
commands=[
 ('focused-final',cargo('suss-cli','clojure_ratios','clojure_bigint_integers','clojure_bigints','read_string','clojure_longs_doubles')),
 ('numeric-controls',cargo('suss-cli','clojure_equality_bitwise','clojure_unboxed_locals','clojure_local_numeric_hints','clojure_raw_numeric_core','clojure_raw_bits_and_casts','clojure_numeric_extrema')),
 ('abi-controls',cargo('suss-compile','runtime_abi','portable_calling_convention','portable_comparator_foundations')),
 ('map-controls',cargo('suss-cli','map_lookups','portable_hash_map_sharing','compiled_macro_hash_maps','compiled_macro_maps')),
 ('source-decoder-controls',cargo('suss-compile','analyzer_edn_core','portable_value_decoder')),
 ('bootstrap',['sh','scripts/verify-bootstrap.sh']),
 ('core-import',['python3','scripts/core_import.py','--check']),
 ('numeric-helper',['python3','scripts/numeric_runtime.py','--check']),
 ('suite-comparator',['python3','-m','unittest','discover','-s','scripts','-p','test_clojure_test_suite.py']),
]
paths=subprocess.check_output(['git','diff','--name-only',BASE],cwd=ROOT,text=True).splitlines()
paths+=['crates/suss-cli/tests/clojure_ratios.rs','crates/suss-compile/src/runtime_abi/ratios.rs','results/m12-ratio-values/cases.json']
# Logs and receipts are outputs, never frozen implementation inputs.
paths=sorted({p for p in paths if not p.startswith('results/')}|{'results/m12-ratio-values/cases.json'})
inputs={p:hashlib.sha256((ROOT/p).read_bytes()).hexdigest() for p in paths}
record={'base':BASE,'head':subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),'inputs':inputs,'commands':[],'complete':False}
def save():(OUT/'commands.json').write_text(json.dumps(record,indent=2)+'\n')
save()
for label,command in commands:
 started=time.time()
 with (OUT/(label+'.log')).open('w') as log:r=subprocess.run(['systemd-run','--user','--scope','-p','MemoryMax=4G','--',*command],cwd=ROOT,env=ENV,stdout=log,stderr=subprocess.STDOUT)
 record['commands'].append({'label':label,'command':command,'exit_code':r.returncode,'seconds':time.time()-started});save();print(label,r.returncode,flush=True)
 if r.returncode:raise SystemExit(r.returncode)
assert all(hashlib.sha256((ROOT/p).read_bytes()).hexdigest()==digest for p,digest in inputs.items()),'Inputs changed while gates were running.'
record['complete']=True;save()
