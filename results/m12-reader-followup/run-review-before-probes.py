"""Execute review counterexamples against the frozen, pre-repair production libs.

Uses the already-built native library/CLI; no third Cargo graph is started.
These counterexamples are not a Cargo regression or full workspace pass.
"""
from pathlib import Path
import hashlib,json,os,shutil,subprocess,tempfile,time
ROOT=Path(__file__).resolve().parents[2];BASE=Path(__file__).resolve().parent
OUT=BASE/'before-review-repairs';OUT.mkdir(exist_ok=True)
assert not (OUT/'commands.json').exists()
TARGET=ROOT.parent/'m12-case-followup-target';DEPS=TARGET/'debug/deps'
LIB=DEPS/'libsuss_compile-2874475a5bbbffec.rlib'
TEMP=next(DEPS.glob('libtempfile-5c62426824f6ab23.rlib'))
SCRATCH=Path(tempfile.mkdtemp(prefix='suss-m12-review-before-'))
CLI=SCRATCH/'suss-before';shutil.copy2(TARGET/'debug/suss',CLI)
source=r'''use suss_compile::{portable::ExpansionHost, portable_macros::CompiledMacros,
    portable_session::{Session,SessionOptions}};
fn main() {
 let root=tempfile::tempdir().unwrap();
 std::fs::write(root.path().join("ledger.sus"),"(ns ledger) (def effects (atom []))").unwrap();
 let a=root.path().join("a.sus");let b=root.path().join("b.sus");
 std::fs::write(&a,"(ns a (:require [ledger])) (swap! ledger/effects conj :a) (def value (fn [] (count @ledger/effects)))").unwrap();
 std::fs::write(&b,"(ns b (:require [a])) (throw (suss.bootstrap/error \"later initializer failed\")) (def value (fn [] (a/value)))").unwrap();
 std::fs::write(root.path().join("app.sus"),"(ns app (:require [b])) (defmacro result [] (b/value)) (def value (result))").unwrap();
 let mut session=Session::new_repl_with_options(SessionOptions{source_paths:vec![root.path().to_owned()],..SessionOptions::default()}).unwrap();
 let mut macros=CompiledMacros::new().unwrap();
 session.load_namespace_with_macros("a",&mut macros).unwrap();
 assert!(session.load_namespace_with_macros("app",&mut macros).is_err());
 assert!(macros.artifact_dependencies().is_none());
 std::fs::write(&a,"(ns a) (throw (suss.bootstrap/error \"successful dependency was replayed\"))").unwrap();
 std::fs::write(&b,"(ns b (:require [a])) (def value (fn [] (a/value)))").unwrap();
 session.load_namespace_with_macros("app",&mut macros).unwrap();
 assert!(macros.artifact_dependencies().is_some(),"source cache identities must recover after successful retry");
}
'''
program=OUT/'cache-probe.rs';program.write_text(source)
binary=SCRATCH/'cache-probe'
command=['rustc','--edition=2024',str(program),'-C','opt-level=1','-C','debuginfo=0',
 '--extern','suss_compile='+str(LIB),'--extern','tempfile='+str(TEMP),'-L','dependency='+str(DEPS),'-o',str(binary)]
for p in sorted((TARGET/'debug/build').glob('*/out')):command+=['-L','native='+str(p)]
paths=subprocess.check_output(['git','ls-files','-z'],cwd=ROOT).decode().split('\0')
inputs={p:hashlib.sha256((ROOT/p).read_bytes()).hexdigest() for p in paths if p and not p.startswith('results/') and (ROOT/p).is_file()}
fixtures=BASE/'uuid-runtime-syntax-quote-primary/cases.json';inputs[str(fixtures.relative_to(ROOT))]=hashlib.sha256(fixtures.read_bytes()).hexdigest()
env=os.environ|{'RUSTUP_TOOLCHAIN':'stable','XDG_RUNTIME_DIR':'/run/user/1000'}
record={'head':subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),
 'inputs':inputs,'artifacts':{str(p):hashlib.sha256(p.read_bytes()).hexdigest() for p in [LIB,TEMP,CLI]},
 'rustc':subprocess.check_output(['rustc','--version'],cwd=ROOT,env=env,text=True).strip(),
 'commands':[],'complete':False,'qualification':'Counterexamples linked to existing frozen pre-repair production artifacts; no new Cargo graph or workspace-pass claim.'}
def save():(OUT/'commands.json').write_text(json.dumps(record,indent=2)+'\n')
def run(label,args):
 started=time.time()
 with (OUT/(label+'.stdout')).open('w') as stdout,(OUT/(label+'.stderr')).open('w') as stderr:
  result=subprocess.run(['systemd-run','--user','--scope','-p','MemoryMax=2G','--',*args],cwd=ROOT,env=env,stdout=stdout,stderr=stderr)
 item={'label':label,'command':args,'exit_code':result.returncode,'seconds':time.time()-started};record['commands'].append(item);save();print(item,flush=True);return result.returncode
save()
if run('build-cache-probe',command)==0:
 run('cache-recovery-before',[str(binary)])
 row=json.loads(fixtures.read_text())[0]
 run('uuid-runtime-syntax-quote-before',[str(CLI),'--no-cache','-e',row['source']])
record.update(complete=True,inputs_unchanged=all(hashlib.sha256((ROOT/p).read_bytes()).hexdigest()==h for p,h in inputs.items()))
save();print('counterexample receipt complete; inspect native failures separately',record['inputs_unchanged'],flush=True)
