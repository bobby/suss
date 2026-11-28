#!/usr/bin/env python3
"""Compile the typed guest with the ordinary CLI and supported default budgets."""
import argparse
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[2]
p = argparse.ArgumentParser(description=__doc__)
p.add_argument('--suss', type=Path, help='use an already built native suss executable')
p.add_argument('--output', type=Path, default=ROOT / 'target/usermanager/application.wasm')
a = p.parse_args()
a.output.parent.mkdir(parents=True, exist_ok=True)
command = [str(a.suss.resolve())] if a.suss else ['cargo', '+stable', 'run', '--locked', '-p', 'suss-cli', '--bin', 'suss', '--']
command += ['compile', 'examples/usermanager/src/usermanager/app.cljc', '-w', 'examples/usermanager/api.wit', '--wit-world', 'usermanager', '-o', str(a.output.resolve())]
for name, source in [('open','open'),('handle','request'),('changes','changes'),('close','close')]:
    command += ['--export', f'suss:usermanager/application@0.1.0#{name}=usermanager.app/{source}']
for name in ['open','key','users','departments','get-user','put-department','insert-user','update-user','delete-user']:
    command += ['--import', f'suss:usermanager/storage@0.1.0#{name}=usermanager.storage/{name}']
subprocess.run(command, cwd=ROOT, check=True)
