"""Record actual EDN-reader results and diagnostics for every UUID parser input."""
from pathlib import Path
import hashlib,json,subprocess,time
BASE=Path(__file__).resolve().parent
old=json.loads((BASE/'reader-oracle-command.json').read_text())
source=BASE/'uuid-runtime-oracle.clj'
source.write_text(r'''(require 'clojure.edn)
(assert (= "1.12.6" (clojure-version)))
(defn quote-units [text]
  (str "\"" (apply str (map #(format "\\u%04x" (int %)) text)) "\""))
(let [rows (read-string (slurp (first *command-line-args*)))
      results
      (mapv (fn [units]
        (let [text (String. (char-array (map char units)))
              tagged (str "#uuid " (quote-units text))
              expression (str "(try (let [u (suss.edn/read-string " (pr-str tagged)
                              ")] [(str u) (hash u)]) (catch :default e [:error (ex-message e)]))")
              result (try (let [u (clojure.edn/read-string tagged)] [(str u) (hash u)])
                          (catch Throwable e [:error (ex-message e)]))]
          (str "{\"expression\":" (quote-units expression) ",\"expected_units\":"
               "[" (clojure.string/join "," (map int (pr-str result))) "]}"))) rows)]
  (spit (second *command-line-args*) (str "[" (clojure.string/join ",\n" results) "]\n"))
  (println (count rows)))
''')
cmd=old['command'][:-1]+[str(source),str(BASE/'uuid-parser-inputs.edn'),str(BASE/'uuid-runtime-cases.json')];start=time.time();r=subprocess.run(cmd,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True)
(BASE/'uuid-runtime-oracle.log').write_text(r.stdout)
d={'command':cmd,'exit_code':r.returncode,'seconds':time.time()-start,'source_sha256':hashlib.sha256(source.read_bytes()).hexdigest(),'jars':old['jars']}
if r.returncode==0:d['cases']=len(json.loads((BASE/'uuid-runtime-cases.json').read_text()));d['output_sha256']=hashlib.sha256((BASE/'uuid-runtime-cases.json').read_bytes()).hexdigest()
(BASE/'uuid-runtime-oracle-command.json').write_text(json.dumps(d,indent=2)+'\n');print(r.stdout);raise SystemExit(r.returncode)
