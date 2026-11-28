#!/usr/bin/env python3
"""Execute production allocator unit tests with explicitly identified prebuilt tools.

The normal acceptance route remains cargo test -p suss-compile --lib command::memory::tests.
This narrow probe avoids a duplicate dependency build while other frozen worktrees
run their workspace baselines. It imports the actual production Rust module.
"""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess

ROOT=Path(__file__).resolve().parents[2]
parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--dependencies',type=Path,required=True)
parser.add_argument('--receipt',type=Path,required=True)
args=parser.parse_args()
dependencies=args.dependencies.resolve()
libraries={name:max(dependencies.glob('lib'+name+'-*.rlib'),key=lambda p:p.stat().st_mtime) for name in ['wasmtime','wasm_encoder']}
harness=ROOT/'target/probe/memory-module-harness.rs'
binary=ROOT/'target/probe/memory-module-harness'
command=['rustc','--edition=2024','--test','-C','opt-level=1',str(harness),'-L','dependency='+str(dependencies),'-o',str(binary)]
for name,path in libraries.items():command+=['--extern',name+'='+str(path)]
record={'command':command,'versions':{tool:subprocess.check_output([tool,'--version'],text=True) for tool in ['rustc','cargo']},
        'inputs':{str(p):hashlib.sha256(p.read_bytes()).hexdigest() for p in [harness,ROOT/'crates/suss-compile/src/portable/command/memory.rs',ROOT/'Cargo.lock',*libraries.values()]},
        'qualification':'Production Rust module imported directly; prebuilt third-party rlibs explicitly identified. This is not a Cargo/workspace baseline.'}
args.receipt.write_text(json.dumps(record,indent=2)+'\n')
result=subprocess.run(command,cwd=ROOT)
if not result.returncode:
 record['binary_sha256']=hashlib.sha256(binary.read_bytes()).hexdigest()
 test=[str(binary),'--test-threads=2','--nocapture']
 record['test_command']=test
 result=subprocess.run(test,cwd=ROOT)
record['exit_code']=result.returncode
record['changed_inputs']=[name for name,sha in record['inputs'].items() if hashlib.sha256(Path(name).read_bytes()).hexdigest()!=sha]
args.receipt.write_text(json.dumps(record,indent=2)+'\n')
raise SystemExit(result.returncode)
