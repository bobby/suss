from pathlib import Path
import json,hashlib,subprocess,time
root=Path(__file__).parent/'m12-numeric-completion';out=root/'results/m12-pattern-values/reader-primary';out.mkdir(exist_ok=False)
tokens=['#""','#"my regex"',r'#"a\"b"',r'#"a\\\"b"',r'#"a\\"',r'#"\d+\s"',r'#"\Q\"\E"',r'#"\u0041"',r'#"\uD800"','#"😀"','#"a\nb"',r'#"(?i)a"','#"["','#"\\','#"unterminated','^:bad #"a"','#_#"[" 0','#?(:absent #"[" :default 0)','#?(:absent #"a" :default 0)','#_#"a" 0']
units=[[int.from_bytes(raw[i:i+2],'little') for i in range(0,len(raw),2)] for raw in [x.encode('utf-16-le','surrogatepass') for x in tokens]]
inputs=out/'tokens.edn';inputs.write_text(json.dumps(units).replace(',',' '));source=out/'oracle.clj';source.write_text('''(assert (= "1.12.6" (clojure-version)))
(let [rows (mapv (fn [units]
  (try
    (let [value (read-string {:read-cond :allow :features #{:suss}} (String. (char-array (map char units))))]
      (if (instance? java.util.regex.Pattern value)
        {:units units :valid true :kind :pattern :pattern-units (mapv int (.pattern value)) :flags (.flags value) :printed-units (mapv int (pr-str value))}
        {:units units :valid true :kind :data :printed (pr-str value)}))
    (catch Throwable error {:units units :valid false :message (ex-message error)})))
  (read-string (slurp (first *command-line-args*))))]
  (spit (second *command-line-args*) (pr-str rows))
  (println (pr-str {:cases (count rows) :accepted (count (filter :valid rows)) :rejected (count (remove :valid rows))})))
''');prior=json.loads((root/'results/m12-decimal-tooling/primary/command.json').read_text());command=['java','-Xmx256m','-cp',':'.join(prior['jars']),'clojure.main',str(source),str(inputs),str(out/'cases.edn')];start=time.time()
with (out/'stdout.log').open('w') as stdout,(out/'stderr.log').open('w') as stderr:p=subprocess.run(command,stdout=stdout,stderr=stderr)
r={'command':command,'exit_code':p.returncode,'seconds':time.time()-start,'controls':len(tokens),'jars':prior['jars'],'source_sha256':hashlib.sha256(source.read_bytes()).hexdigest(),'input_sha256':hashlib.sha256(inputs.read_bytes()).hexdigest(),'qualification':'Fresh pinned regex literal reader observations, including escapes, exact source/flags, rejected metadata, discards and inactive branches. No native reader pass is inferred.'}
if (out/'cases.edn').exists():r['output_sha256']=hashlib.sha256((out/'cases.edn').read_bytes()).hexdigest()
(out/'command.json').write_text(json.dumps(r,indent=2)+'\n');print({k:v for k,v in r.items() if k not in ('command','jars')});print((out/'stdout.log').read_text());print((out/'stderr.log').read_text());raise SystemExit(p.returncode)
