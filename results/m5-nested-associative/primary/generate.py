from pathlib import Path
import subprocess,hashlib,json
root=Path.cwd();out=root/'results/m5-nested-associative/primary'
names=['deps.edn','oracle.clj','inputs.json'];inputs={n:hashlib.sha256((out/n).read_bytes()).hexdigest() for n in names}
r=subprocess.run(['clojure','-M','oracle.clj'],cwd=out,capture_output=True)
(out/'stdout.json').write_bytes(r.stdout);(out/'stderr.log').write_bytes(r.stderr)
(out/'receipt.json').write_text(json.dumps({'command':['clojure','-M','oracle.clj'],'exit_code':r.returncode,'inputs':inputs,'stdout_sha256':hashlib.sha256(r.stdout).hexdigest(),'stderr_sha256':hashlib.sha256(r.stderr).hexdigest()},indent=2)+'\n')
if r.returncode:print(r.stderr.decode());raise SystemExit(r.returncode)
cases=json.loads(r.stdout);(out/'cases.json').write_text(json.dumps(cases,indent=2,ensure_ascii=False)+'\n');print(len(cases),'JVM cases')
