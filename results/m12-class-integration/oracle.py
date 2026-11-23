from pathlib import Path
import hashlib,json,subprocess
OUT=Path(__file__).resolve().parent
jars=[Path('/home/bobby/.m2/repository')/p for p in ['org/clojure/clojure/1.12.6/clojure-1.12.6.jar','org/clojure/spec.alpha/0.5.238/spec.alpha-0.5.238.jar','org/clojure/core.specs.alpha/0.4.74/core.specs.alpha-0.4.74.jar']]
cases=[
 '(pr-str [(type 1)])',
 '[(identical? (type 7) (type 8)) (identical? (type 7) (type ##NaN)) (= (type 7) Long) (= (type ##NaN) Double) (ifn? (type 7)) (ifn? (type ##NaN))]',
 '(let [effects (atom 0) c (type 7)] [(try (c (do (swap! effects inc) 7) (do (swap! effects #(+ (* % 10) 2)) 9)) false (catch Throwable _ true)) @effects])',
 '(do (defmulti ty type) (try (ty 1) (catch Throwable e (ex-message e))))',
]
cases += [
 '(let [effects (atom 0) flag (atom true) c (if @flag (type 7) (fn [x] x))] [(try (c (do (swap! effects inc) 7)) false (catch Throwable _ true)) @effects])',
 '(let [effects (atom 0)] [(try ((do (swap! effects inc) (type 7)) (do (swap! effects #(+ (* % 10) 2)) 9)) false (catch Throwable _ true)) @effects])',
 '(let [c (type 7)] (try (c (throw (ex-info "argument" {:arg true}))) (catch Throwable e (ex-data e))))',
 '(let [effects (atom 0) c (type 7)] [(try (apply c [(do (swap! effects inc) 7)]) false (catch Throwable _ true)) @effects])',
 '(let [effects (atom 0) c (identity (fn [x y] x))] [(c (do (swap! effects inc) 7) (do (swap! effects #(+ (* % 10) 2)) 9)) @effects])',
]
for value in ['1','1.0','##NaN','1N','(biginteger 1)','1/3','18446744073709551616N']:
 for tail in ['0','0 :missing']:
  cases.append(f'(try (nth {value} {tail}) (catch Throwable e (ex-message e)))')
command=['java','-cp',':'.join(map(str,jars)),'clojure.main','-e','\n'.join('(prn '+s+')' for s in cases)]
r=subprocess.run(command,text=True,capture_output=True)
(OUT/'oracle.json').write_text(json.dumps({'command':command,'jars':{str(p):hashlib.sha256(p.read_bytes()).hexdigest() for p in jars},'exit_code':r.returncode,'stdout':r.stdout,'stderr':r.stderr},indent=2)+'\n')
assert r.returncode==0,r.stderr
values=r.stdout.splitlines();assert len(values)==len(cases)
rows=[{'source':s.replace('(catch Throwable','(catch :default'),'expected':v} for s,v in zip(cases,values)]
rows[3]['target']='core_state::defmulti_options_and_type_dispatch_follow_clojure'
(OUT/'all-cases.json').write_text(json.dumps(rows,indent=2)+'\n')
# This declaration needs macro expansion; its existing Repl target executes it.
(OUT/'cases.json').write_text(json.dumps([r for r in rows if 'target' not in r],indent=2)+'\n')
print(json.dumps(rows,indent=2))
