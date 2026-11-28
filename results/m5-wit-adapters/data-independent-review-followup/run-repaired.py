import hashlib,json,subprocess,time
from pathlib import Path
root=Path.cwd();out=root/'target/probe/review-524-followup/repaired'
paths=[
'crates/suss-compile/src/portable/aot.rs',
'crates/suss-compile/src/portable/aot/schemas.rs',
'crates/suss-compile/src/portable/aot/composite.rs',
'crates/suss-compile/src/portable/command/shape.rs',
'crates/suss-compile/tests/portable_aot_wit_review_regressions.rs',
'lib:target/probe/cargo/debug/deps/libsuss_compile-eacd3b495e585863.rlib',
'lib:target/probe/cargo/debug/deps/libwasmtime-6dac808114cae8a2.rlib',
'probe:target/probe/review-524-followup/probe-repaired.rs',
'probe:target/probe/review-524-followup/probe-repaired',
]
def hashes():
 return {p:hashlib.sha256((root/p.split(':',1)[-1]).read_bytes()).hexdigest() for p in paths}
head=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip()
record={'head':head,'inputs':hashes(),'command':['target/probe/review-524-followup/probe-repaired'],'rustc':subprocess.check_output(['rustup','run','stable','rustc','--version'],text=True).strip()}
(out/'receipt.json').write_text(json.dumps(record,indent=2)+'\n')
start=time.monotonic()
r=subprocess.run(record['command'],capture_output=True,text=True)
(out/'run.stdout').write_text(r.stdout);(out/'run.stderr').write_text(r.stderr)
record.update(exit_code=r.returncode,seconds=time.monotonic()-start,changed_inputs=[p for p,h in record['inputs'].items() if hashes()[p]!=h],head_after=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip())
(out/'receipt.json').write_text(json.dumps(record,indent=2)+'\n')
print(json.dumps(record),flush=True)
raise SystemExit(r.returncode)
