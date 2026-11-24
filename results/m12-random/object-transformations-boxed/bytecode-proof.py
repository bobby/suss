from pathlib import Path
import hashlib,json,subprocess,time
BASE=Path(__file__).resolve().parent;ROOT=BASE.parents[2]
recorded=json.loads((BASE/'command.json').read_text());jars=recorded['jars'];sha=lambda b:hashlib.sha256(b).hexdigest()
assert all(sha(Path(path).read_bytes())==h for path,h in jars.items())
source=BASE/'rand_object_proof.clj';source.write_text('(ns rand-object-proof)\n(defn compute [^Object bound ^Object draw]\n  (let [^Object product (* bound draw)] (int product)))\n')
classes=ROOT.parent/'m12-some-fn-target/codex-test-temporary/rand-object-overload-bytecode';classes.mkdir(exist_ok=False)
compile_command=['java','-Xmx512m','-cp',str(BASE)+':'+':'.join(jars),'clojure.main','-e','(binding [*compile-path* '+json.dumps(str(classes))+'] (compile (quote rand-object-proof)))']
javap_command=['javap','-classpath',str(classes)+':'+':'.join(jars),'-c','rand_object_proof$compute']
record={'source_sha256':sha(source.read_bytes()),'jars':jars,'commands':[],'qualification':'Development bytecode proof for explicit Object input/product tags: this checks the same multiply(Object,Object) and intCast(Object) signatures as actual rand/rand-int, not a native pass.'}
for name,command in [('compile',compile_command),('javap',javap_command)]:
 start=time.time();r=subprocess.run(command,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
 (BASE/(name+'.stdout')).write_bytes(r.stdout);(BASE/(name+'.stderr')).write_bytes(r.stderr)
 record['commands'].append({'label':name,'command':command,'exit_code':r.returncode,'seconds':time.time()-start,'stdout_sha256':sha(r.stdout)})
 (BASE/'bytecode-command.json').write_text(json.dumps(record,indent=2)+'\n')
 assert r.returncode==0,r.stderr.decode()
text=(BASE/'javap.stdout').read_text();assert 'clojure/lang/Numbers.multiply:(Ljava/lang/Object;Ljava/lang/Object;)' in text and 'clojure/lang/RT.intCast:(Ljava/lang/Object;)I' in text
record['verified_object_signatures']=True;(BASE/'bytecode-command.json').write_text(json.dumps(record,indent=2)+'\n')
print('Generated primary Object transformation uses the same two JVM signatures as rand/rand-int')
