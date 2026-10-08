#!/usr/bin/env python3
"""Verify the isolated, pinned development SDK; no compiler adoption claim."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--archive', required=True, type=Path)
parser.add_argument('--sdk', required=True, type=Path)
args = parser.parse_args()
lock = json.loads((Path(__file__).resolve().parents[1] / 'toolchain-lock.json').read_text())
sha = hashlib.sha256()
with args.archive.open('rb') as archive:
    for chunk in iter(lambda: archive.read(1024 * 1024), b''):
        sha.update(chunk)
if args.archive.stat().st_size != lock['archive_bytes'] or sha.hexdigest() != lock['archive_sha256']:
    raise SystemExit('LLVM archive differs from the pinned release checksum/size')
required = ['bin/mlir-opt', 'bin/mlir-tblgen', 'bin/clang++', 'bin/ld64.lld',
            'include/mlir/IR/MLIRContext.h', 'lib/libMLIRIR.a',
            'lib/cmake/mlir/MLIRConfig.cmake', 'lib/cmake/llvm/LLVMConfig.cmake',
            'include/llvm/Support/LICENSE.TXT']
for relative in required:
    if not (args.sdk / relative).is_file():
        raise SystemExit(f'missing required SDK file: {relative}')
version = subprocess.run([str(args.sdk / 'bin/mlir-opt'), '--version'],
                         check=True, capture_output=True, text=True, timeout=30).stdout
expected = lock['release'].removeprefix('llvmorg-')
if f'LLVM version {expected}\n' not in version:
    raise SystemExit('installed mlir-opt version differs from the pinned release')
dialects = subprocess.run([str(args.sdk / 'bin/mlir-opt'), '--show-dialects'],
                          check=True, capture_output=True, text=True, timeout=30).stdout
if 'wasmssa' not in dialects.replace(',', ' ').split():
    raise SystemExit('installed MLIR package lacks the expected wasmssa dialect')
print(json.dumps({'release': lock['release'], 'source_commit': lock['source_commit'],
                  'archive_sha256': sha.hexdigest(), 'required_sdk_files': required,
                  'version': version.strip(), 'wasmssa_registered': True}, indent=2))
