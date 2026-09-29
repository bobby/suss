#!/usr/bin/env python3
"""Run M0's browser fixture in an isolated headless Chrome profile on localhost."""
import argparse
import functools
import hashlib
import html
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
import json
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
from threading import Thread

ROOT = Path(__file__).resolve().parents[1]
FEATURES = ('gc', 'function-references', 'tail-call', 'exceptions')

REQUIRED_CHECKS = ('required core features execute', 'feature compile rejection diagnostic',
                   'ES module loading', 'WasmGC continuation',
                   'Promise suspension and resume', 'cancellation cleanup',
                   'stale callback isolation')
JCO_CHECKS = ('transpiled ES module import', 'component export returns 42',
              'core GC payload executes')


def validate_result(result, require_jco=False):
    if not isinstance(result, dict) or result.get('status') != 'passed':
        raise ValueError('browser probe did not pass')
    if result.get('features') != list(FEATURES):
        raise ValueError('missing or changed required core feature results')
    if result.get('checks') != list(REQUIRED_CHECKS):
        raise ValueError('missing or changed browser acceptance checks')
    expected = {'status': 'passed', 'checks': list(JCO_CHECKS)} if require_jco else {'status': 'not-requested'}
    if result.get('jco') != expected:
        raise ValueError('missing or unexpected Jco packaging result')


class QuietHandler(SimpleHTTPRequestHandler):
    def log_message(self, *args):
        pass


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--chrome', required=True)
    parser.add_argument('--wasm-tools', default='wasm-tools')
    parser.add_argument('--output', type=Path)
    parser.add_argument('--jco', help='optional pinned Jco CLI path; absence is not a passing Jco result')
    args = parser.parse_args()
    version = subprocess.check_output([args.chrome, '--version'], text=True).strip()
    tools_version = subprocess.check_output([args.wasm_tools, '--version'], text=True).strip()
    with tempfile.TemporaryDirectory(prefix='suss-browser-') as directory:
        temp = Path(directory)
        site = temp / 'site'
        shutil.copytree(ROOT / 'tests/browser', site)
        subprocess.run([args.wasm_tools, 'parse', str(site / 'continuation.wat'), '-o',
                        str(site / 'continuation.wasm')], check=True)
        for feature in FEATURES:
            subprocess.run([args.wasm_tools, 'parse',
                            str(ROOT / 'tests/toolchain' / (feature + '.wat')),
                            '-o', str(site / (feature + '.wasm'))], check=True)
        jco_version = None
        generated_hashes = None
        if args.jco:
            jco_version = subprocess.check_output([args.jco, '--version'], text=True).strip()
            component = temp / 'gc-component.wasm'
            subprocess.run([args.wasm_tools, 'parse', str(site / 'jco-gc-component.wat'),
                            '-o', str(component)], check=True)
            subprocess.run([args.jco, 'transpile', str(component), '--name', 'gc-probe',
                            '--instantiation', 'async', '--no-nodejs-compat',
                            '-o', str(site / 'jco')], check=True, stdout=subprocess.PIPE)
            generated_hashes = {p.name: hashlib.sha256(p.read_bytes()).hexdigest()
                                for p in sorted((site / 'jco').iterdir()) if p.is_file()}
        handler = functools.partial(QuietHandler, directory=str(site))
        server = ThreadingHTTPServer(('127.0.0.1', 0), handler)
        worker = Thread(target=server.serve_forever, daemon=True)
        worker.start()
        stopped_after_output = False
        try:
            run = subprocess.run([
                args.chrome, '--headless', '--disable-gpu', '--no-first-run',
                '--no-default-browser-check', '--disable-background-networking',
                '--disable-component-update', '--disable-sync',
                '--user-data-dir=' + str(temp / 'profile'), '--dump-dom',
                '--virtual-time-budget=10000',
                f'http://127.0.0.1:{server.server_port}/index.html' + ('?jco=1' if args.jco else ''),
            ], capture_output=True, text=True, timeout=20)
        except subprocess.TimeoutExpired as error:
            # Some macOS Chrome builds emit the completed DOM then hang during
            # display teardown. subprocess.run already killed the isolated
            # process. Accept only an explicit completed fixture result below,
            # and record the process-lifecycle limitation in the evidence.
            stdout = error.stdout or b''
            stderr = error.stderr or b''
            run = subprocess.CompletedProcess(error.cmd, None,
                stdout.decode() if isinstance(stdout, bytes) else stdout,
                stderr.decode() if isinstance(stderr, bytes) else stderr)
            stopped_after_output = True
        finally:
            server.shutdown()
            server.server_close()
            worker.join()
        match = re.search(r'<pre id="result">(.*?)</pre>', run.stdout, re.S)
        if run.returncode or not match:
            raise SystemExit(f'Chrome probe failed ({run.returncode}): {run.stderr[-2000:]}')
        try:
            result = json.loads(html.unescape(match.group(1)))
        except json.JSONDecodeError:
            raise SystemExit('browser did not complete the probe: ' + match.group(1))
    validation_error = None
    try:
        validate_result(result, require_jco=bool(args.jco))
    except ValueError as error:
        validation_error = str(error)
    evidence = {'schema': 1, 'semantic_probe_status': 'failed' if validation_error else 'passed',
                'validation_error': validation_error, 'browser': version, 'wasm-tools': tools_version,
                'scope': 'single-browser feasibility fixture; not Suss browser output or canonical ABI',
                'result': result, 'jco_version': jco_version, 'jco_generated_sha256': generated_hashes,
                'browser_terminated_after_dom_timeout': stopped_after_output,
                'browser_process_exit_code': run.returncode,
                'source_sha256': {
                    str(p.relative_to(ROOT)): hashlib.sha256(p.read_bytes()).hexdigest()
                    for p in sorted([ROOT / 'scripts/probe_browser.py'] + list((ROOT / 'tests/browser').iterdir()) +
                                    [ROOT / 'tests/toolchain' / (name + '.wat') for name in FEATURES])
                    if p.is_file()
                }}
    output = json.dumps(evidence, indent=2) + '\n'
    if args.output:
        args.output.write_text(output)
    else:
        print(output, end='')
    if validation_error:
        raise SystemExit(validation_error)


if __name__ == '__main__':
    main()
