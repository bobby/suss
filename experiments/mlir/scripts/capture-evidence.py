#!/usr/bin/env python3
"""Record content identities; never infer test success from an existing log."""
import argparse
import hashlib
import json
from pathlib import Path
import platform
import subprocess

ROOT = Path(__file__).resolve().parents[3]
EXPERIMENT = ROOT / 'experiments/mlir'
parser = argparse.ArgumentParser()
parser.add_argument('--sdk', type=Path)
parser.add_argument('--build', type=Path)
parser.add_argument('--bridge', type=Path)
parser.add_argument('--native', type=Path)
parser.add_argument('--check', type=Path)
args = parser.parse_args()


def identity(path):
    return {'bytes': path.stat().st_size,
            'sha256': hashlib.sha256(path.read_bytes()).hexdigest()}


files = {str(path.relative_to(ROOT)): identity(path)
         for path in sorted(EXPERIMENT.rglob('*'))
         if path.is_file() and 'evidence' not in path.relative_to(EXPERIMENT).parts
         and '__pycache__' not in path.parts and 'target' not in path.parts}
bootstrap = {str(path.relative_to(ROOT)): identity(path)
             for path in sorted((ROOT / 'runtime/bootstrap').glob('*'))
             if path.suffix in {'.wasm', '.json'}}
production_paths = [ROOT / 'Cargo.toml', ROOT / 'Cargo.lock']
for crate in ['suss-core', 'suss-reader', 'suss-compile', 'suss-cli']:
    production_paths.append(ROOT / 'crates' / crate / 'Cargo.toml')
    production_paths.extend(path for path in (ROOT / 'crates' / crate / 'src').rglob('*') if path.is_file())
production = {str(path.relative_to(ROOT)): identity(path) for path in sorted(production_paths)}
if args.check:
    observed = json.loads(args.check.read_text())
    if observed['experiment_files'] != files or observed['bootstrap_files'] != bootstrap or observed['production_files'] != production:
        raise SystemExit('experiment, bootstrap or reused production identities differ from retained evidence')
    print('PASS complete experiment, bootstrap and reused production content identities')
    raise SystemExit(0)
if not all([args.sdk, args.build, args.bridge, args.native]):
    parser.error('capture requires --sdk, --build, --bridge and --native')


def output(command):
    return subprocess.check_output([str(part) for part in command], text=True).strip()


binaries = {'bridge': args.bridge, 'native_cli': args.native,
            'exporter': args.build / 'suss-mlir-export', 'opt': args.build / 'suss-mlir-opt'}
record = {
    'schema': 'suss.mlir.evidence-identity.v1',
    'base_commit': output(['git', '-C', ROOT, 'rev-parse', 'HEAD']),
    'binding': 'experiment content hashes are authoritative across the subsequent commit; no self-referential commit hash',
    'experiment_files': files, 'bootstrap_files': bootstrap, 'production_files': production,
    'root_lock': identity(ROOT / 'Cargo.lock'),
    'binaries': {name: {'path': str(path.resolve()), **identity(path)} for name, path in binaries.items()},
    'tool_versions': {
        'llvm': output([args.sdk / 'bin/mlir-opt', '--version']),
        'clang': output([args.sdk / 'bin/clang++', '--version']),
        'rustc': output(['rustc', '--version']), 'cargo': output(['cargo', '--version']),
        'node': output(['/opt/homebrew/bin/node', '--version']),
        'python': platform.python_version(), 'platform': platform.platform()},
    'intended_build_profiles': {
        'cpp': {'generator': 'Unix Makefiles', 'build_type': '', 'workers': 1, 'compiler': 'pinned Clang23', 'linker': 'pinned ld64.lld'},
        'rust_dev': {'debug': 0, 'incremental': False},
        'rust_test': {'debug': 0, 'incremental': False, 'dependency_opt_level': 2,
                      'dependency_debug_assertions': True, 'dependency_overflow_checks': True},
        'root_test_command': 'cargo test --workspace --locked -- --test-threads=2'},
    'upstream_definitions': {name: identity(args.sdk / 'include/mlir/Dialect/WasmSSA/IR' / name)
                             for name in ['WasmSSAOps.td', 'WasmSSATypes.td']},
    'profile_limits': 'Rust profiles are declared from tracked Cargo.toml, not an audit of externally managed compiler flags; effective CMake values separately recorded',
    'effective_cmake_cache': {line.split(':', 1)[0]: line.split('=', 1)[1]
        for line in (args.build / 'CMakeCache.txt').read_text().splitlines()
        if line.startswith(('CMAKE_BUILD_TYPE:', 'CMAKE_C_COMPILER:', 'CMAKE_CXX_COMPILER:',
                            'CMAKE_CXX_FLAGS:', 'CMAKE_EXE_LINKER_FLAGS:', 'CMAKE_OSX_SYSROOT:',
                            'CMAKE_GENERATOR:', 'MLIR_DIR:', 'LLVM_DIR:'))},
    'test_status': 'not inferred here; explicit command/status records and retained terminal logs required',
    'ci_scope': 'root CI does not execute isolated Rust workspace or LLVM/C++ gates'}
print(json.dumps(record, indent=2))
