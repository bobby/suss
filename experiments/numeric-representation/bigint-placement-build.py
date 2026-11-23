"""Build the original, non-shipped #338 placement probe with pinned inputs."""
from pathlib import Path
import os,subprocess,json,re,hashlib,time
root=Path(__file__).resolve().parents[2];outer=root.parents[2];source=root/'experiments/numeric-representation';out=root/'results/m12-bigint-placement';work=root/'target/probe/bigint-placement';work.mkdir(parents=True,exist_ok=True)
binpath=Path(os.environ.get('SUSS_BINARYEN_BIN',outer/'target/probe/m11-binaryen/binaryen-version_133/bin'))
commands=[]
def run(command,env=None):
 start=time.time();r=subprocess.run(list(map(str,command)),cwd=root,env=env,capture_output=True,text=True);commands.append({'command':list(map(str,command)),'exit_code':r.returncode,'seconds':time.time()-start,'stdout':r.stdout,'stderr':r.stderr});(out/'build-commands.json').write_text(json.dumps(commands,indent=2)+'\n');assert r.returncode==0,r.stderr
run(['cargo','+1.98.0','build','--manifest-path',source/'bigint-helper/Cargo.toml','--release','--target','wasm32-unknown-unknown','--locked'],os.environ|{'CARGO_TARGET_DIR':str(outer/'target/probe/m12-bigint-helper-target')})
helper=outer/'target/probe/m12-bigint-helper-target/wasm32-unknown-unknown/release/bigint_helper.wasm'
run([binpath/'wasm-dis',helper,'-o',work/'helper.wat'])
s=(work/'helper.wat').read_text();globals=re.findall(r'\(global (\$[^ ]+) \(mut i32\) \(i32.const (\d+)\)\)',s);assert len(globals)==1,globals
name,top=globals[0];s=s.replace('(module','(module\n (export "stack" (global '+name+'))',1);(work/'helper-stack.wat').write_text(s)
run([binpath/'wasm-as','--all-features','--disable-custom-descriptors',work/'helper-stack.wat','-o',work/'helper-stack.wasm'])
run(['python3',source/'src/bigint-generate.py',top])
run([binpath/'wasm-as','--all-features','--disable-custom-descriptors',source/'src/bigint-generated.wat','-o',work/'gc.wasm'])
run([binpath/'wasm-merge',work/'helper-stack.wasm','kernel',work/'gc.wasm','bench','--all-features','--disable-custom-descriptors','-o',work/'merged.wasm'])
print('built merged placement module',flush=True)
