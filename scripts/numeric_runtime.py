#!/usr/bin/env python3
"""Verify/rebuild the source-pinned, allocator-free numeric Wasm build input."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1] / 'runtime' / 'numeric'
INPUTS = ('Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml', 'src/lib.rs',
          'licenses/ryu-js-APACHE.txt', 'licenses/ryu-js-BOOST.txt',
          'licenses/Rust-1.98.0-COPYRIGHT-library.html', 'NOTICE')
TOOLCHAIN = {'rustc_release': '1.98.0',
             'rustc_commit': '88d9e12ae178fab0fb5cc050a94da85685d449ea',
             'target': 'wasm32-unknown-unknown', 'profile': 'release'}
FORMATTER = {'crate': 'ryu-js', 'version': '1.0.3',
             'registry_checksum': '04d056b875a9d2e6cb9a61d127afee9ac5999b9f87bcb32079d1318e505be714',
             'licenses': ['Apache-2.0', 'BSL-1.0']}


def unique(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError(f'duplicate numeric manifest field: {key}')
        result[key] = value
    return result


def digest(data):
    return hashlib.sha256(data).hexdigest()


def inputs(root):
    return {name: digest((root / name).read_bytes()) for name in INPUTS}


def manifest(root, binary):
    if binary[:8] != b'\x00asm\x01\x00\x00\x00':
        raise ValueError('numeric artifact must be a core Wasm module')
    return {'schema': 1, 'toolchain': TOOLCHAIN, 'formatter': FORMATTER,
            'wasm_sha256': digest(binary), 'inputs': inputs(root)}


def verify(root=ROOT):
    recorded = json.loads((root / 'artifact/manifest.json').read_text(),
                          object_pairs_hook=unique)
    expected = manifest(root, (root / 'artifact/numeric.wasm').read_bytes())
    # Exact schema and types: Python True must not stand in for schema integer 1.
    if type(recorded.get('schema')) is not int or recorded != expected:
        raise ValueError('stale or malformed numeric artifact manifest; rebuild and review source/artifact changes')
    return expected


def rebuild(root=ROOT):
    before = inputs(root)
    version = subprocess.check_output(['rustc', '--version', '--verbose'], cwd=root, text=True)
    if f"release: {TOOLCHAIN['rustc_release']}\n" not in version or f"commit-hash: {TOOLCHAIN['rustc_commit']}\n" not in version:
        raise ValueError('wrong numeric helper Rust toolchain')
    with tempfile.TemporaryDirectory(prefix='suss-numeric-rebuild-') as target:
        env = dict(os.environ, CARGO_TARGET_DIR=target)
        # Keep the developer's environment flags intact. No RUSTFLAGS override.
        subprocess.run(['cargo', 'build', '--release', '--target', TOOLCHAIN['target'], '--locked'],
                       cwd=root, env=env, check=True)
        binary = (Path(target) / TOOLCHAIN['target'] / 'release/suss_numeric_intrinsics.wasm').read_bytes()
    if inputs(root) != before:
        raise ValueError('numeric sources changed during rebuild')
    manifest(root, binary)
    return binary


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument('--record', action='store_true', help='rebuild, then record source/binary hashes; review every diff')
    mode.add_argument('--rebuild-check', action='store_true', help='rebuild with the pinned Rust compiler and compare bytes')
    mode.add_argument('--check', action='store_true', help='verify tracked source, dependency, license and Wasm hashes (default)')
    args = parser.parse_args()
    if args.record:
        binary = rebuild()
        (ROOT / 'artifact/numeric.wasm').write_bytes(binary)
        (ROOT / 'artifact/manifest.json').write_text(json.dumps(manifest(ROOT, binary), indent=2) + '\n')
    else:
        verify()
        if args.rebuild_check and rebuild() != (ROOT / 'artifact/numeric.wasm').read_bytes():
            raise ValueError('numeric helper rebuild differs from tracked Wasm; do not silently replace it')
    print('Numeric source/artifact manifest verified' if not args.record else 'Numeric artifact rebuilt and recorded; review the diff')


if __name__ == '__main__':
    main()
