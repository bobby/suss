#!/usr/bin/env python3
"""Verify byte-exact official WIT sources offline; optionally resolve with wasm-tools.

The package/reference scan records identities only. Upstream wit-parser (through
wasm-tools) is the syntax and world-resolution authority, not this scanner.
"""
import argparse
import hashlib
import json
import re
import subprocess
import tempfile
import tarfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SOURCES = ROOT / 'vendor/wasi/wasi-wit-0.3.1'
LOCK = ROOT / 'docs/roadmap/wasi-wit-lock.json'


def sha256(data):
    return hashlib.sha256(data).hexdigest()


def manifest(root):
    files, packages = {}, {}
    for path in sorted(root.rglob('*.wit')):
        data = path.read_bytes()
        source = re.sub(r'//[^\n]*|/\*.*?\*/', '', data.decode('utf-8'), flags=re.S)
        declaration = re.search(r'^\s*package\s+([^;\s]+)\s*;', source)
        if not declaration:
            raise ValueError(f'missing package declaration: {path}')
        package = declaration[1]
        references = sorted(set(re.findall(r'\b(wasi:[\w-]+)/[\w-]+@([0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?)', source)))
        dependencies = sorted({f'{name}@{version}' for name, version in references} - {package})
        relative = path.relative_to(root).as_posix()
        files[relative] = {'sha256': sha256(data), 'package': package, 'dependencies': dependencies}
        if len(path.relative_to(root).parts) == 2:
            if package in packages:
                raise ValueError(f'duplicate top-level package: {package}')
            packages[package] = {'path': path.parent.relative_to(root).as_posix(),
                                 'dependencies': dependencies}
    for package, info in packages.items():
        prefix = info['path'] + '/deps/'
        available = {f['package'] for name, f in files.items() if name.startswith(prefix)} | {package}
        required = set(info['dependencies'])
        for name, entry in files.items():
            if name.startswith(prefix):
                required.update(entry['dependencies'])
        missing = required - available
        if missing:
            raise ValueError(f'{package}: missing local dependencies {sorted(missing)}')
    return {'files': files, 'packages': packages}


def verify(root, lock):
    actual = manifest(root)
    for key in ('files', 'packages'):
        if actual[key] != lock[key]:
            raise ValueError(f'WIT lock mismatch in {key}; review source changes, do not silently regenerate')
    return actual


def verify_archive(path, lock):
    if sha256(path.read_bytes()) != lock['archive_sha256']:
        raise ValueError('official release archive hash mismatch')
    files = {}
    prefix = 'wasi-wit-0.3.1/'
    with tarfile.open(path, 'r:gz') as archive:
        for member in archive.getmembers():
            if member.isdir():
                continue
            if not member.isfile() or not member.name.startswith(prefix):
                raise ValueError(f'unexpected archive member: {member.name}')
            name = member.name[len(prefix):]
            if name in files:
                raise ValueError(f'duplicate archive member: {name}')
            files[name] = sha256(archive.extractfile(member).read())
    expected = {name: entry['sha256'] for name, entry in lock['files'].items()}
    if files != expected:
        raise ValueError('archive content differs from vendored WIT lock')


def probe(root, lock, tool):
    version = subprocess.check_output([tool, '--version'], text=True).strip()
    if not re.fullmatch(r'wasm-tools 1\.258\.0(?: \([^\n]+\))?', version):
        raise ValueError(f'expected wasm-tools 1.258.0, got {version}')
    results = {}
    with tempfile.TemporaryDirectory(prefix='suss-wit-') as tmp:
        for package, info in sorted(lock['packages'].items()):
            path = root / info['path']
            command = [tool, 'component', 'wit', str(path)]
            resolved = json.loads(subprocess.check_output(command + ['--json'], text=True, timeout=30))
            identities = sorted(p['name'] for p in resolved['packages'])
            expected = sorted({entry['package'] for name, entry in lock['files'].items()
                               if name.startswith(info['path'] + '/')})
            if identities != expected:
                raise ValueError(f'{package}: resolved package identities changed')
            binary = Path(tmp) / (info['path'] + '.wasm')
            subprocess.run(command + ['--wasm', '-o', str(binary)], check=True, timeout=30)
            # --wasm validates the package encoding; decode it again independently.
            decoded = json.loads(subprocess.check_output(
                [tool, 'component', 'wit', str(binary), '--json'], text=True, timeout=30))
            if sorted(p['name'] for p in decoded['packages']) != identities:
                raise ValueError(f'{package}: binary round-trip identities changed')
            worlds = sorted(w['name'] for w in resolved['worlds']
                            if resolved['packages'][w['package']]['name'] == package)
            decoded_worlds = sorted(w['name'] for w in decoded['worlds']
                                    if decoded['packages'][w['package']]['name'] == package)
            if worlds != decoded_worlds:
                raise ValueError(f'{package}: binary round-trip worlds changed')
            results[package] = {'resolved_packages': identities, 'worlds': worlds,
                                'resolution': 'passed', 'validated_binary_roundtrip': 'passed'}
    return {'tool': version, 'source_lock_sha256': sha256(LOCK.read_bytes()),
            'packages': results, 'scope': 'WIT resolution and validated package encoding only; no capability execution'}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--wasm-tools')
    parser.add_argument('--archive', type=Path, help='Verify an already downloaded official release archive')
    parser.add_argument('--output', type=Path)
    args = parser.parse_args()
    if args.output and not args.wasm_tools:
        parser.error('--output requires --wasm-tools')
    lock = json.loads(LOCK.read_text())
    verify(SOURCES, lock)
    if args.archive:
        verify_archive(args.archive, lock)
    print(f"Verified {len(lock['files'])} WIT files and {len(lock['packages'])} official packages")
    if args.wasm_tools:
        result = probe(SOURCES, lock, args.wasm_tools)
        if args.output:
            args.output.write_text(json.dumps(result, indent=2) + '\n')
        print(f"Resolved and binary-roundtripped {len(result['packages'])} packages")


if __name__ == '__main__':
    main()
