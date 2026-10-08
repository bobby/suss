#!/usr/bin/env python3
"""Reproducible physical source footprint, not a complexity or replacement claim."""
import hashlib
import json
from pathlib import Path

EXP = Path(__file__).resolve().parents[1]
ROOT = EXP.parents[1]
groups = {
    'additional_handwritten_mlir_cpp_tablegen': sorted((EXP / 'src').glob('*')),
    'additional_handwritten_bridge_rust': sorted((EXP / 'bridge/src').glob('*.rs')),
    'additional_experiment_python': sorted(EXP.rglob('*.py')),
    'reused_native_portable_frontend_ir_emitter': sorted((ROOT / 'crates/suss-compile/src/portable').rglob('*.rs')),
    'reused_native_runtime_abi': [ROOT / 'crates/suss-compile/src/runtime_abi.rs'] + sorted((ROOT / 'crates/suss-compile/src/runtime_abi').rglob('*.rs')),
}
report = {}
for name, paths in groups.items():
    files = {}
    for path in paths:
        if not path.is_file():
            continue
        raw = path.read_bytes()
        lines = raw.decode().splitlines()
        files[str(path.relative_to(ROOT))] = {
            'bytes': len(raw), 'physical_lines': len(lines),
            'nonblank_lines': sum(bool(line.strip()) for line in lines),
            'sha256': hashlib.sha256(raw).hexdigest(),
        }
    report[name] = {
        'file_count': len(files),
        **{key: sum(item[key] for item in files.values())
           for key in ['bytes', 'physical_lines', 'nonblank_lines']},
        'files': files,
    }
print(json.dumps({
    'schema': 'suss.mlir.implementation-footprint.v1',
    'method': 'UTF-8 bytes and splitlines, including comments; nonblank separately',
    'excluded': ['generated TableGen/CMake files', 'LLVM/MLIR implementation',
                 'Rust dependency implementation', 'docs and fixture data',
                 'shipped reader/CLI/macro hosts and tests outside experiment'],
    'limitations': ['native groups cover substantially more behavior than the experiment',
                    'MLIR layer adds to, and does not replace, the reused native groups',
                    'line counts do not measure ongoing maintenance savings'],
    'groups': report,
}, indent=2))
