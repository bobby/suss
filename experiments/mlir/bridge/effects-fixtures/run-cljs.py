#!/usr/bin/env python3
# Original isolated oracle driver; repository MIT/Apache-2.0 terms.
import argparse,json,os,shutil,subprocess
from pathlib import Path
P=Path(__file__).parent
PIN='c4295f303100bbf5afac449242d30bca1126f1a1'
a=argparse.ArgumentParser();a.add_argument('--upstream',type=Path,required=True);a.add_argument('--workdir',type=Path,required=True);a.add_argument('--native',type=Path);args=a.parse_args()
up=args.upstream.resolve();work=args.workdir.resolve()
assert subprocess.check_output(['git','-C',str(up),'rev-parse','HEAD'],text=True).strip()==PIN
subprocess.run(['git','-C',str(up),'diff','--quiet','HEAD','--','src/main'],check=True)
assert subprocess.check_output(['/opt/homebrew/bin/node','--version'],text=True).strip()=='v24.5.0'
assert (up/'src/main/cljs/cljs/core.cljs').is_file()
(work/'src/suss_oracle').mkdir(parents=True,exist_ok=True)
shutil.copyfile(P/'mlir_effects.cljs',work/'src/suss_oracle/mlir_effects.cljs')
(work/'deps.edn').write_text('{:paths ["src"] :mvn/local-repo "/tmp/suss-oracle-m2" :deps {org.clojure/clojure {:mvn/version "1.12.1"} org.clojure/clojurescript {:local/root '+json.dumps(str(up))+'}}}\n')
env=dict(os.environ,CLJ_CONFIG='/tmp/suss-oracle-clojure-config',CLJ_CACHE=str(work/'clj-cache'),JAVA_TOOL_OPTIONS='-Xmx512m')
options='{:force true :cache-analysis false :target :nodejs :output-to "out/effects.js" :output-dir "out/cljs" :optimizations :none :source-map false}'
r=subprocess.run(['clojure','-Srepro','-M','-m','cljs.main','-co',options,'-c','suss-oracle.mlir-effects'],cwd=work,env=env,capture_output=True,text=True,timeout=180)
(work/'compile.log').write_text(r.stdout+r.stderr);assert r.returncode==0,r.stdout+r.stderr
r=subprocess.run(['/opt/homebrew/bin/node','out/effects.js'],cwd=work,capture_output=True,text=True,timeout=15);assert r.returncode==0,r.stderr
(work/'observations.json').write_text(r.stdout)
cmd=['python3',str(P/'check-cljs.py'),str(work/'observations.json')]
if args.native:cmd+=['--native',str(args.native.resolve())]
subprocess.run(cmd,check=True,timeout=240)
