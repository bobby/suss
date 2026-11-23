from pathlib import Path
import hashlib,json,subprocess,time
ROOT=Path(__file__).resolve().parents[2];OUT=Path(__file__).resolve().parent
source=r"""(require '[clojure.edn :as edn] '[clojure.java.io :as io])
(assert (= "1.12.6" (clojure-version)))
(let [files (filter #(.endsWith (.getName %) ".edn") (file-seq (io/file "results/m12-decimal-conversions/ci-repair/aot-size-after")))]
  (assert (seq files))
  (doseq [f files]
    (let [text (slurp f) m (edn/read-string text)]
      (assert (= 13 (count m)))
      (assert (= 2 (:format-version m)))
      (assert (= 12 (:runtime-abi m)))
      (assert (every? #(re-matches #"[a-f0-9]{64}" %) (keep m [:compiler-source-sha256 :source-sha256 :wasm-sha256])))
      (assert (= m (edn/read-string (pr-str m))))
      (when (seq (:macro-dependencies m))
        (assert (some #(= "macro:suss.core/with-precision" (first %)) (:macro-dependencies m)))
        (assert (every? #(re-matches #"[a-f0-9]{64}" (second %)) (:macro-dependencies m))))))
  (println "Clojure EDN read and print/read:" (count files) "actual emitted manifests"))
"""
p=OUT/'compact-edn.clj';p.write_text(source)
prior=json.loads((ROOT/'results/m12-wide-wit/oracle-command.json').read_text());command=prior['command'][:-1]+[str(p)]
t=time.time();r=subprocess.run(command,cwd=ROOT,text=True,capture_output=True)
(OUT/'compact-edn.log').write_text(r.stdout+r.stderr)
(OUT/'compact-edn-command.json').write_text(json.dumps({'command':command,'exit_code':r.returncode,'seconds':time.time()-t,'source_sha256':hashlib.sha256(source.encode()).hexdigest(),'manifests':{str(p.relative_to(ROOT)):hashlib.sha256(p.read_bytes()).hexdigest() for p in (OUT/'ci-repair/aot-size-after').glob('*.edn')},'jars':{p:hashlib.sha256(Path(p).read_bytes()).hexdigest() for p in prior['jars']}},indent=2)+'\n')
print(r.stdout+r.stderr);assert r.returncode==0
