#!/usr/bin/env python3
"""Execute the identical original/mutated source in pinned CLJS and existing Suss.
Native CLI observations here are explicit printed scalars; the bridge's separate
WasmGC gate independently decodes boxed binary64 values.
"""
import argparse
import json
import os
from pathlib import Path
import subprocess

P = Path(__file__).parent
PIN = 'c4295f303100bbf5afac449242d30bca1126f1a1'
parser = argparse.ArgumentParser()
parser.add_argument('--upstream', type=Path, required=True)
parser.add_argument('--workdir', type=Path, required=True)
parser.add_argument('--suss', type=Path, required=True)
args = parser.parse_args()
up, work = args.upstream.resolve(), args.workdir.resolve()
assert subprocess.check_output(['git', '-C', str(up), 'rev-parse', 'HEAD'], text=True).strip() == PIN
subprocess.run(['git', '-C', str(up), 'diff', '--quiet', 'HEAD', '--', 'src/main'], check=True)
source = (P / 'closure.sus').read_text()
programs = [source, source.replace('x 7', 'x 11')]
assert programs[0] != programs[1]
(work / 'src/suss_oracle').mkdir(parents=True, exist_ok=True)
(work / 'deps.edn').write_text('{:paths ["src"] :mvn/local-repo "/tmp/suss-oracle-m2" :deps {org.clojure/clojure {:mvn/version "1.12.1"} org.clojure/clojurescript {:local/root ' + json.dumps(str(up)) + '}}}\n')
entry = '''(ns suss-oracle.mlir-source)
(defn bits [value]
  (let [buffer (js/ArrayBuffer. 8) view (js/DataView. buffer)]
    (.setFloat64 view 0 value false)
    (.padStart (.toString (.getBigUint64 view 0 false) 16) 16 "0")))
(defn -main [] (println (.stringify js/JSON (into-array [(bits SOURCE0) (bits SOURCE1)]))))
(set! *main-cli-fn* -main)
'''.replace('SOURCE0', programs[0]).replace('SOURCE1', programs[1])
(work / 'src/suss_oracle/mlir_source.cljs').write_text(entry)
env = dict(os.environ, CLJ_CONFIG='/tmp/suss-oracle-clojure-config',
           CLJ_CACHE=str(work / 'clj-cache'), JAVA_TOOL_OPTIONS='-Xmx512m')
options = '{:force true :cache-analysis false :target :nodejs :output-to "out/source.js" :output-dir "out/cljs" :optimizations :none :source-map false}'
result = subprocess.run(['clojure', '-Srepro', '-M', '-m', 'cljs.main', '-co', options,
                         '-c', 'suss-oracle.mlir-source'], cwd=work, env=env,
                        capture_output=True, text=True, timeout=180)
(work / 'compile.log').write_text(result.stdout + result.stderr)
assert result.returncode == 0, result.stderr
result = subprocess.run(['node', 'out/source.js'], cwd=work, capture_output=True,
                        text=True, timeout=15, check=True)
expected = ['4022000000000000', '402a000000000000']
assert json.loads(result.stdout) == expected, result.stdout
(work / 'cljs-observations.json').write_text(result.stdout)
native = []
for index, program in enumerate(programs):
    path = work / ('source-' + str(index) + '.sus')
    path.write_text(program)
    result = subprocess.run([str(args.suss.resolve()), str(path)], capture_output=True,
                            text=True, timeout=60, check=True)
    assert result.stdout.strip() == ['9', '13'][index], result.stdout
    native.append(result.stdout.strip())
(work / 'native-observations.json').write_text(json.dumps(native) + '\n')
print('PASS identical original/mutated source: pinned CLJS binary64 9/13 and existing Suss CLI printed 9/13')
