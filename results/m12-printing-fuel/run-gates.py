"""Execute the unchanged printer regression first, then its neighboring gates."""
from pathlib import Path
import hashlib, json, os, subprocess, time
ROOT = Path(__file__).resolve().parents[2]
OUT = Path(__file__).resolve().parent
ENV = os.environ | {'CARGO_TARGET_DIR': str(ROOT.parent/'m12-decimal-target'),
                    'CARGO_BUILD_JOBS': '2', 'RUSTUP_TOOLCHAIN': 'stable',
                    'XDG_RUNTIME_DIR': '/run/user/1000'}
paths = subprocess.check_output(['git','ls-files','-z'], cwd=ROOT).decode().split('\0')
inputs = {p: hashlib.sha256((ROOT/p).read_bytes()).hexdigest() for p in paths
          if p and not p.startswith('results/') and (ROOT/p).is_file()}
record = {'base': subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),
          'inputs': inputs, 'commands': [], 'complete': False}
def save():
    (OUT/'commands.json').write_text(json.dumps(record,indent=2)+'\n')
def run(label, command):
    assert all(hashlib.sha256((ROOT/p).read_bytes()).hexdigest()==h for p,h in inputs.items())
    started=time.time()
    with (OUT/(label+'.log')).open('w') as log:
        r=subprocess.run(['systemd-run','--user','--scope','-p','MemoryMax=4G','--',*command],
                         cwd=ROOT,env=ENV,stdout=log,stderr=subprocess.STDOUT)
    record['commands'].append({'label':label,'command':command,'exit_code':r.returncode,'seconds':time.time()-started})
    save(); print(label,r.returncode,flush=True)
    if r.returncode: raise SystemExit(r.returncode)
save()
run('printing-chunks',['cargo','test','--locked','-p','suss-cli','--test','printing_chunks','--','--test-threads=2'])
run('printing-neighbors',['cargo','test','--locked','-p','suss-cli','--test','collection_printing',
    '--test','printer_options','--test','object_printing','--test','core_import','--','--test-threads=2'])
run('bootstrap',['sh','scripts/verify-bootstrap.sh'])
run('core-import',['python3','scripts/core_import.py','--check'])
record.update(complete=True,inputs_unchanged=all(hashlib.sha256((ROOT/p).read_bytes()).hexdigest()==h for p,h in inputs.items()))
save()
