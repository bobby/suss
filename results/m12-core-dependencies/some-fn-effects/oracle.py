"""Fresh primary controls for construction/argument effects and captures."""
from pathlib import Path
import hashlib,json,subprocess,time
BASE=Path(__file__).resolve().parent
cases=[]
def add(name,expression):
    cases.append({"name":"some-fn-effects-"+name,"expression":expression})
add("construction-order", '(let [t (atom []) f (some-fn (do (swap! t conj :make-a) (fn [x] (swap! t conj [:a x]) false)) (do (swap! t conj :make-b) (fn [x] (swap! t conj [:b x]) :hit))) v (f 1 2)] [v @t])')
add("construction-throw", '(let [t (atom []) v (try (some-fn (do (swap! t conj :make-a) (throw (ex-info "stop" {:v :stopped}))) (do (swap! t conj :make-b) identity)) (catch Throwable e (:v (ex-data e))))] [v @t])')
add("argument-order", '(let [t (atom []) f (some-fn (fn [x] (swap! t conj [:p x]) :hit)) v (f (do (swap! t conj :arg-a) 1) (do (swap! t conj :arg-b) 2))] [v @t])')
add("argument-throw", '(let [t (atom []) f (some-fn (fn [x] (swap! t conj [:p x]) :hit)) v (try (f (do (swap! t conj :arg-a) 1) (do (swap! t conj :arg-b) (throw (ex-info "stop" {:v :stopped}))) (do (swap! t conj :arg-c) 3)) (catch Throwable e (:v (ex-data e))))] [v @t])')
add("predicate-throw", '(let [t (atom []) f (some-fn (fn [x] (swap! t conj [:a x]) (throw (ex-info "stop" {:v :stopped}))) (fn [x] (swap! t conj [:b x]) :hit)) v (try (f (do (swap! t conj :arg-a) 1) (do (swap! t conj :arg-b) 2)) (catch Throwable e (:v (ex-data e))))] [v @t])')
add("captured-original-function", '(do (def some-fn-original-p (fn [x] [:old x])) (let [f (some-fn some-fn-original-p)] (def some-fn-original-p (fn [x] [:new x])) (f 42)))')
add("retained-capture", '(let [captured 41 f (some-fn (fn [x] (if (= x captured) [:captured x] nil)))] [(f 0 41) (f 42)])')
add("zero-argument-construction", '(let [t (atom []) f (some-fn (do (swap! t conj :make) (fn [x] (swap! t conj x) :hit))) v (f)] [v @t])')
source=BASE/"oracle.clj"
source.write_text('(assert (= "1.12.6" (clojure-version)))\n'+''.join('(println (pr-str (pr-str '+r["expression"]+')))\n' for r in cases))
parent=json.loads((BASE.parent/"oracle-command.json").read_text())
jars=parent["jars"]
assert all(hashlib.sha256(Path(p).read_bytes()).hexdigest()==h for p,h in jars.items())
command=["java","-Xmx512m","-cp",":".join(jars),"clojure.main",str(source)]
started=time.time();result=subprocess.run(command,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
(BASE/"stdout.log").write_bytes(result.stdout);(BASE/"stderr.log").write_bytes(result.stderr)
values=[json.loads(line) for line in result.stdout.decode().splitlines()]
record={"command":command,"exit_code":result.returncode,"seconds":time.time()-started,"jars":jars,"source_sha256":hashlib.sha256(source.read_bytes()).hexdigest(),"stdout_sha256":hashlib.sha256(result.stdout).hexdigest(),"rows":len(values)}
(BASE/"command.json").write_text(json.dumps(record,indent=2)+"\n")
assert result.returncode==0 and len(values)==len(cases)==8,result.stderr.decode()
for row,value in zip(cases,values):row["expected"]=value
(BASE/"cases.json").write_text(json.dumps(cases,indent=2)+"\n")
print("Fresh Clojure1.12.6 construction/argument/capture controls",len(cases))
