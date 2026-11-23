"""Fresh primary multiplication values for independent GC-field checks."""
from pathlib import Path
import hashlib,json,random,subprocess,time
ROOT=Path(__file__).resolve().parents[2];OUT=Path(__file__).resolve().parent
rng=random.Random(338071)
shapes=[(0,0),(0,8),(1,1),(2,2),(2,8),(3,3),(3,4),(4,4),(1,512),(512,1),(2,512),(512,2),(32,128),(128,32),(512,512)]
pairs=[]
for la,lb in shapes:
 a=[rng.getrandbits(32) for _ in range(la)];b=[rng.getrandbits(32) for _ in range(lb)]
 if a:a[-1]|=1<<31
 if b:b[-1]|=1<<31
 pairs.append((a,b))
pairs += [([0xffffffff]*4,[0xffffffff]*4),([1,0,0,1],[1,0,0,1])]
def integer(limbs):return sum(x<<(32*i) for i,x in enumerate(limbs))
def limbs(n):return [(n>>(32*i))&0xffffffff for i in range((n.bit_length()+31)//32)]
source='(assert (= "1.12.6" (clojure-version)))\n'
for a,b in pairs:
 source+=f'(println (pr-str (.toString (biginteger (* (bigint (java.math.BigInteger. "{integer(a):x}" 16)) (bigint (java.math.BigInteger. "{integer(b):x}" 16)))) 16)))\n'
p=OUT/'oracle.clj';p.write_text(source)
prior=json.loads((ROOT/'results/m12-wide-wit/oracle-command.json').read_text());command=prior['command'][:-1]+[str(p)]
start=time.time();r=subprocess.run(command,cwd=ROOT,capture_output=True,text=True);(OUT/'oracle.log').write_text(r.stdout+r.stderr)
(OUT/'oracle-command.json').write_text(json.dumps({'command':command,'source_sha256':hashlib.sha256(source.encode()).hexdigest(),'jars':{p:hashlib.sha256(Path(p).read_bytes()).hexdigest() for p in prior['jars']},'exit_code':r.returncode,'seconds':time.time()-start},indent=2)+'\n')
assert r.returncode==0,r.stderr
observed=[json.loads(line) for line in r.stdout.splitlines()];assert len(observed)==len(pairs)
cases=[]
for (a,b),actual in zip(pairs,observed):
 expected=integer(a)*integer(b);assert actual==f'{expected:x}'
 cases.append({'a':a,'b':b,'product':limbs(expected)})
(OUT/'cases.json').write_text(json.dumps(cases,indent=2)+'\n');print(len(cases),'fresh primary magnitude products')
