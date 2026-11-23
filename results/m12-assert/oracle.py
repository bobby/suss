"""Pinned Clojure assert source and fresh control-flow/message observations."""
from pathlib import Path
import hashlib,json,subprocess,time,zipfile
BASE=Path(__file__).resolve().parent
PARENT=BASE.parent/"m12-core-dependencies"
sha=lambda raw:hashlib.sha256(raw).hexdigest()
upstream=json.loads((PARENT/"source-provenance.json").read_text())
jar=Path(upstream["jar"]);assert sha(jar.read_bytes())==upstream["jar_sha256"]
with zipfile.ZipFile(jar) as z:core=z.read(upstream["entry"])
assert sha(core)==upstream["entry_sha256"]
start=core.index(b"(defmacro assert\n");end=core.index(b"\n(defn test\n",start)
original=core[start:end].rstrip()+b"\n"
(BASE/"upstream-assert.clj").write_bytes(original)
cases=[]
def add(name,expression):cases.append({"name":name,"expression":expression})
for name,value in [("true","true"),("zero","0"),("string",'""'),("vector","[]"),("map","{}"),("keyword",":ok")]:
    add("truthy-"+name,"(assert "+value+")")
for name,value in [("false","false"),("nil","nil")]:
    add("message-"+name,'(try (assert '+value+') (catch Throwable e (ex-message e)))')
for name,message in [("string",'"details"'),("nil","nil"),("keyword",":details"),("number","42")]:
    add("optional-"+name,'(try (assert false '+message+') (catch Throwable e (ex-message e)))')
add("condition-once", '(let [t (atom []) v (assert (do (swap! t conj :condition) true) (do (swap! t conj :message) "unused"))] [v @t])')
add("failed-condition-before-message", '(let [t (atom []) v (try (assert (do (swap! t conj :condition) false) (do (swap! t conj :message) "details")) (catch Throwable e (ex-message e)))] [v @t])')
add("condition-throw", '(let [t (atom []) v (try (assert (do (swap! t conj :condition) (throw (ex-info "condition" {:v :condition-threw}))) (do (swap! t conj :message) "unused")) (catch Throwable e (:v (ex-data e))))] [v @t])')
add("message-throw", '(let [t (atom []) v (try (assert (do (swap! t conj :condition) false) (do (swap! t conj :message) (throw (ex-info "message" {:v :message-threw})))) (catch Throwable e (:v (ex-data e))))] [v @t])')
add("fresh-errors", '(let [a (try (assert false) (catch Throwable e e)) b (try (assert false) (catch Throwable e e))] [(identical? a b) (ex-message a) (ex-message b) (ex-data a)])')
add("lexical-shadow", '(let [assert (fn [x] [:local x])] (assert false))')
add("runtime-binding-is-too-late", '(try (binding [*assert* false] (assert false)) (catch Throwable e (ex-message e)))')
source=BASE/"oracle.clj"
source.write_text('(assert (= "1.12.6" (clojure-version)))\n'+''.join('(println (pr-str (pr-str '+row["expression"]+')))\n' for row in cases)+
    '(spit (first *command-line-args*) (pr-str [*assert* (.isDynamic #\'*assert*) (binding [*assert* false] (macroexpand \'(assert definitely-missing (also-missing))))]))\n')
jars=json.loads((PARENT/"oracle-command.json").read_text())["jars"]
assert all(sha(Path(p).read_bytes())==h for p,h in jars.items())
command=["java","-Xmx512m","-cp",":".join(jars),"clojure.main",str(source),str(BASE/"compile-time.edn")]
started=time.time();r=subprocess.run(command,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
(BASE/"stdout.log").write_bytes(r.stdout);(BASE/"stderr.log").write_bytes(r.stderr)
record={"command":command,"exit_code":r.returncode,"seconds":time.time()-started,"jars":jars,"upstream":upstream["upstream"],"entry":upstream["entry"],"entry_sha256":upstream["entry_sha256"],"original_sha256":sha(original),"source_sha256":sha(source.read_bytes()),"stdout_sha256":sha(r.stdout)}
(BASE/"command.json").write_text(json.dumps(record,indent=2)+"\n")
assert r.returncode==0,r.stderr.decode()
values=[json.loads(line) for line in r.stdout.decode().splitlines()];assert len(values)==len(cases)==19
for row,value in zip(cases,values):row["expected"]=value
(BASE/"cases.json").write_text(json.dumps(cases,indent=2)+"\n")
assert (BASE/"compile-time.edn").read_text()=="[true true nil]"
print("Fresh Clojure assert observations",len(cases),"plus compile-time flag/default/dynamic data")
