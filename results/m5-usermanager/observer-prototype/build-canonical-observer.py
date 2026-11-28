from pathlib import Path
import subprocess,json,hashlib
root=Path.cwd();deps=root/'target/probe/cargo/debug/deps';libs={}
for crate,package in [('wasm_encoder','wasm-encoder'),('wasmparser','wasmparser')]:
 for path in deps.glob(f'{crate}-*.d'):
  if f'/{package}-0.258.0/' in path.read_text():
   lib=deps/f'lib{path.stem}.rlib'
   if lib.exists():libs[crate]=lib;break
assert len(libs)==2,libs
command=['rustup','run','stable','rustc','--edition=2024','target/probe/canonical-stats-main.rs','-L',f'dependency={deps}']
for crate,path in libs.items():command += ['--extern',f'{crate}={path}']
command += ['-o','target/probe/canonical-stats-main']
r=subprocess.run(command,capture_output=True)
Path('target/probe/canonical-observer-build.stdout').write_bytes(r.stdout);Path('target/probe/canonical-observer-build.stderr').write_bytes(r.stderr)
print(r.returncode,r.stderr.decode()[-3000:]);print({k:str(v) for k,v in libs.items()})
if r.returncode:raise SystemExit(r.returncode)
r=subprocess.run(['target/probe/canonical-stats-main',str(root.parent/'m5-composites/target/probe/review-524-followup/fixed-packed.wasm'),'target/probe/fixed-packed-canonical-observer.wasm'],capture_output=True)
Path('target/probe/canonical-observer-run.stdout').write_bytes(r.stdout);Path('target/probe/canonical-observer-run.stderr').write_bytes(r.stderr)
print(r.returncode,r.stdout.decode(),r.stderr.decode()[-4000:])
