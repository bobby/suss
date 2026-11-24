"""Pure primary rand transformations plus separately qualified real random samples."""
from pathlib import Path
import hashlib,json,subprocess,time
BASE=Path(__file__).resolve().parent
provenance=json.loads((BASE/"source-provenance.json").read_text())
jars=provenance["jars"];sha=lambda raw:hashlib.sha256(raw).hexdigest()
assert all(sha(Path(p).read_bytes())==h for p,h in jars.items())
bounds=["0","1","-1","5","-5","5.5","-5.5","##NaN","##Inf","##-Inf","2147483648","4294967296","1N","1/3","1.25M","nil","true",'"5"',":bad","[]","{}"]
controls=[]
for bound in bounds:
 for draw in ["0.0","0.5","0.9999999999999999"]:
  for operation in ["rand","rand-int"]:
   value="(* "+bound+" "+draw+")"
   if operation=="rand-int":value="(int "+value+")"
   expression='(try (let [x '+value+'] [:value (pr-str x) (cond (double? x) :double (integer? x) :integer :else :other)]) (catch Throwable e [:error]))'
   controls.append({"operation":operation,"bound":bound,"draw":draw,"expression":expression})
samples=[]
for operation in ["rand","rand-int"]:
 for bound in bounds:
  expression='(try (let [x ('+operation+' '+bound+')] [:value (pr-str x) (.getName (class x))]) (catch Throwable e [:error (.getName (class e)) (ex-message e)]))'
  samples.append({"operation":operation,"bound":bound,"expression":expression})
source=BASE/"oracle.clj"
source.write_text('(assert (= "1.12.6" (clojure-version)))\n'+''.join('(println (pr-str (pr-str '+r["expression"]+')))\n' for r in controls+samples)+
 '(println (pr-str (pr-str (vec (repeatedly 16 rand)))))\n')
command=["java","-Xmx512m","-cp",":".join(jars),"clojure.main",str(source)]
started=time.time();r=subprocess.run(command,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
(BASE/"stdout.log").write_bytes(r.stdout);(BASE/"stderr.log").write_bytes(r.stderr)
record={"command":command,"exit_code":r.returncode,"seconds":time.time()-started,"jars":jars,"source_sha256":sha(source.read_bytes()),"stdout_sha256":sha(r.stdout),"qualification":"126 exact expectations isolate the multiplication and int-cast steps of the retained primary rand/rand-int source using controlled draws. JVM bytecode directly invokes static rand implementations, so with-redefs is not a draw-control mechanism. The42 real public calls and16-draw stream are nondeterministic observations, not native exact-value goldens. Native public rand/provider behavior requires separate execution."}
(BASE/"command.json").write_text(json.dumps(record,indent=2)+"\n")
assert r.returncode==0,r.stderr.decode()
values=[json.loads(line) for line in r.stdout.decode().splitlines()]
assert len(controls)==126 and len(samples)==42 and len(values)==169
for row,value in zip(controls,values[:126]):row["expected"]=value
for row,value in zip(samples,values[126:168]):row["actual_primary_sample"]=value
(BASE/"controlled-transformations.json").write_text(json.dumps(controls,indent=2)+"\n")
(BASE/"actual-public-samples.json").write_text(json.dumps({"calls":samples,"rand_stream":values[-1],"qualification":"Nondeterministic primary observations only; not exact native expected samples."},indent=2)+"\n")
print("Primary controlled transformations",len(controls),"real public calls",len(samples),"plus16 real draws")
