from pathlib import Path
import os,subprocess,time,json,hashlib
base=Path(__file__).resolve().parent
out=base/'refined'
out.mkdir(exist_ok=False)
root=base.parent.parent/'native-host-var-foundation'
deps=Path('/home/bobby/.codex-workspaces/worktrees/ios-cd7ccbc9-1c87-43fe-95e7-2b6b71168849/suss/target/probe/m12-some-fn-target/debug/deps')
lib=deps/'libsuss_compile-d24d0e8fe0a40796.rlib'
binroot=deps.parent/'suss'
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
files=[base/'refined_probe.rs',lib,binroot,root/'crates/suss-compile/src/portable_macros.rs',root/'crates/suss-compile/src/portable_session/evaluation/native_values/operations.rs',root/'crates/suss-compile/src/portable_session/evaluation/native_values/macro_vars.rs']
head=subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip()
fingerprints={str(p):sha(p) for p in files}
record={'review_head':head,'implementation_head':'4b3a95c531c6809d1e7f391b80504bc89200e49b','qualification':'Independent exploratory publication-cut probe linked against existing implementation library; no Cargo or source/test edits.','input_sha256':fingerprints,'commands':[],'complete':False}
def save():(out/'commands.json').write_text(json.dumps(record,indent=2)+'\n')
binaryen=Path('/home/bobby/.codex-workspaces/worktrees/ios-cd7ccbc9-1c87-43fe-95e7-2b6b71168849/suss/target/probe/m11-binaryen/binaryen-version_133/bin')
env=os.environ|{'BINARYEN_CORES':'1','SUSS_WASM_OPT':str(binaryen/'wasm-opt'),'SUSS_BUILD_DIR':str(root/'target/suss')}
env['PATH']=str(binaryen)+os.pathsep+env['PATH']
commands=[('compile',['rustup','run','stable','rustc','--edition=2024',str(base/'refined_probe.rs'),'--extern','suss_compile='+str(lib),'-L','dependency='+str(deps),'-o',str(out/'probe')]),('execute',[str(out/'probe'),str(root)])]
save()
for name,command in commands:
 row={'name':name,'command':command,'complete':False};record['commands'].append(row);save();start=time.monotonic()
 with (out/(name+'.stdout')).open('w') as stdout,(out/(name+'.stderr')).open('w') as stderr:
  process=subprocess.Popen(command,cwd=out,env=env,stdout=stdout,stderr=stderr);row['pid']=process.pid;save();code=process.wait()
 row.update(complete=True,exit_code=code,seconds=time.monotonic()-start);save()
 if code:break
record.update(complete=True,input_unchanged={str(p):sha(p)==fingerprints[str(p)] for p in files},head_unchanged=head==subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip());save()
print(json.dumps({k:v for k,v in record.items() if k!='input_sha256'},indent=2))
print('\n'.join((out/(name+'.stdout')).read_text().splitlines()[-8:]));print((out/(name+'.stderr')).read_text());raise SystemExit(code)
