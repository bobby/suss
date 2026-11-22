from pathlib import Path
import hashlib,json,subprocess,time
out=Path(__file__).resolve().parent
forms=[
 '(case 1N 1 :yes :no)', '(case 1 1N :yes :no)',
 '(case 1/3 1/3 :yes :no)', '(case 1/3 2/6 :yes :no)',
 '(let [x 1/3] [(identical? x (abs x)) (identical? (numerator x) (numerator x)) (identical? (denominator x) (denominator x))])',
 '(let [x -1/3] (identical? x (abs x)))',
 '(let [f /] [(f 1 3) (f 6 3) (f 6N 3)])',
 '(let [x 3/2] (let [^long y x] [y (+ y 1)]))',
 '(rationalize nil)', '(rationalize "x")',
 '(unchecked-byte 8589934595/2)', '(unchecked-short 8589934595/2)',
 '(try (long 18446744073709551617/2) (catch :default e (.-message e)))',
 '(try (short 65537/2) (catch :default e (.-message e)))',
 '(try (byte 257/2) (catch :default e (.-message e)))',
]
observed=[f'(pr-str (try {form} (catch :default e :error)))' for form in forms]
source='(assert (= "1.12.6" (clojure-version)))\n'+'\n'.join('(println (pr-str '+form.replace('(catch :default e','(catch Throwable e').replace('(.-message e)','(.getMessage e)')+'))' for form in observed)+'\n'
(out/'edge-oracle.clj').write_text(source)
old=json.loads((out/'oracle-command.json').read_text());command=old['command'][:-1]+[str(out/'edge-oracle.clj')];started=time.time();r=subprocess.run(command,capture_output=True,text=True)
(out/'edge-oracle.log').write_text(r.stdout+r.stderr);(out/'edge-oracle-command.json').write_text(json.dumps({'command':command,'exit_code':r.returncode,'seconds':time.time()-started,'source_sha256':hashlib.sha256(source.encode()).hexdigest(),'jars':old['jars']},indent=2)+'\n');assert r.returncode==0,r.stderr
values=[json.loads(line) for line in r.stdout.splitlines()];assert len(values)==len(forms)
(out/'edge-cases.json').write_text(json.dumps(list(zip(observed,values)),indent=2)+'\n')
for form,want in zip(forms,values):print(form,want)
