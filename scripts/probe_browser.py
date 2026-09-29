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


class QuietHandler(SimpleHTTPRequestHandler):
    def log_message(self, *args):
        pass


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--chrome', required=True)
    parser.add_argument('--wasm-tools', default='wasm-tools')
    parser.add_argument('--output', type=Path)
    args = parser.parse_args()
    version = subprocess.check_output([args.chrome, '--version'], text=True).strip()
    tools_version = subprocess.check_output([args.wasm_tools, '--version'], text=True).strip()
    with tempfile.TemporaryDirectory(prefix='suss-browser-') as directory:
        temp = Path(directory)
        site = temp / 'site'
        shutil.copytree(ROOT / 'tests/browser', site)
        subprocess.run([args.wasm_tools, 'parse', str(site / 'continuation.wat'), '-o',
                        str(site / 'continuation.wasm')], check=True)
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
                f'http://127.0.0.1:{server.server_port}/index.html',
            ], capture_output=True, text=True, timeout=20)
        except subprocess.TimeoutExpired as error:
            # Some macOS Chrome builds emit the completed DOM then hang during
            # display teardown. subprocess.run already killed the isolated
            # process. Accept only an explicit completed fixture result below,
            # and record the process-lifecycle limitation in the evidence.
            stdout = error.stdout or b''
            stderr = error.stderr or b''
            run = subprocess.CompletedProcess(error.cmd, 0,
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
    evidence = {'schema': 1, 'browser': version, 'wasm-tools': tools_version,
                'scope': 'single-browser feasibility fixture; not Suss browser output or canonical ABI',
                'result': result, 'browser_terminated_after_dom_timeout': stopped_after_output,
                'source_sha256': {
                    p.name: hashlib.sha256(p.read_bytes()).hexdigest()
                    for p in sorted((ROOT / 'tests/browser').iterdir()) if p.is_file()
                }}
    output = json.dumps(evidence, indent=2) + '\n'
    if args.output:
        args.output.write_text(output)
    else:
        print(output, end='')
    if result['status'] != 'passed':
        raise SystemExit(1)


if __name__ == '__main__':
    main()
