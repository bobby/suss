"""Replace former linear-case assumptions only with fresh JVM observations."""
from pathlib import Path
import hashlib,json,subprocess,time
OUT=Path(__file__).resolve().parent
selectors=['false','nil','0','2','"seven"','true',':late',':other','"tail"',':missing']
expressions=[]
for selector in selectors:
 expressions.append(f'''(let [order (atom []) selector-count (atom 0) arms (atom 0) original =
 value (with-redefs [= (fn [left right] (swap! order conj [left right]) (original left right))]
   (case (do (swap! selector-count inc) {selector})
    (false nil 0 1 2 :hit "seven" true :late) (do (swap! arms #(+ (* % 10) 1)) 17)
    :other (do (swap! arms #(+ (* % 10) 2)) 19)
    (3 "tail") (do (swap! arms #(+ (* % 10) 3)) 21)
    (do (swap! arms #(+ (* % 10) 4)) 23)))]
 [value @selector-count @arms @order])''')
collision='''(let [order (atom []) selector-count (atom 0) arms (atom 0) original =
 result (try (with-redefs [= (fn [left right] (swap! order conj left)
  (if (original left 0) (throw (ex-info "late equality" {:value 91})) (original left right)))]
  (case (do (swap! selector-count inc) 0)
   (false nil 0 1 2 :hit "seven" true :late) (do (reset! arms 1) 17)
   :other (do (reset! arms 2) 19) (do (reset! arms 3) 23)))
  (catch clojure.lang.ExceptionInfo e [:thrown (ex-message e) (ex-data e)]))]
 [result @selector-count @arms @order (identical? original =)])'''
expressions.append(collision)
duplicates=['(case :late (false nil 0 1 2 :hit "seven" true :late) 17 :late 19 23)', '(case :missing (false nil 0 1 2 :hit "seven" true :late) 17 nil 19 23)', '(case 0 (false nil 0 1 2 :hit "seven" true 0) 17 23)']
for form in duplicates:expressions.append("(try (macroexpand '"+form+") (catch Throwable e (loop [e e] (if (ex-cause e) (recur (ex-cause e)) (ex-message e)))))")
src=OUT/'legacy-case-tests-oracle.clj';src.write_text('(assert (= "1.12.6" (clojure-version)))\n'+''.join('(println (pr-str (pr-str '+e+')))\n' for e in expressions))
jar=Path.home()/'.m2/repository/org/clojure/clojure/1.12.6/clojure-1.12.6.jar';jars=[jar,Path.home()/'.m2/repository/org/clojure/spec.alpha/0.5.238/spec.alpha-0.5.238.jar',Path.home()/'.m2/repository/org/clojure/core.specs.alpha/0.4.74/core.specs.alpha-0.4.74.jar'];cmd=['java','-cp',':'.join(str(p) for p in jars),'clojure.main',str(src)];start=time.time();r=subprocess.run(cmd,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True);(OUT/'legacy-case-tests-oracle.log').write_text(r.stdout);(OUT/'legacy-case-tests-command.json').write_text(json.dumps({'command':cmd,'exit_code':r.returncode,'seconds':time.time()-start,'source_sha256':hashlib.sha256(src.read_bytes()).hexdigest(),'jars':{str(p):hashlib.sha256(p.read_bytes()).hexdigest() for p in jars}},indent=2)+'\n');assert r.returncode==0,r.stdout
outputs=[json.loads(line) for line in r.stdout.splitlines()];assert len(outputs)==len(expressions)
fixture={'mixed':[{'selector':s,'expected':v} for s,v in zip(selectors,outputs)],'collision':outputs[len(selectors)],'duplicate_messages':outputs[len(selectors)+1:]}
(OUT/'legacy-case-primary.json').write_text(json.dumps(fixture,indent=2)+'\n');print(json.dumps(fixture,indent=2))
