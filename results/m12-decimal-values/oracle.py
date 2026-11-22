"""Primary Java BigDecimal formatting observations through pinned Clojure."""
from pathlib import Path
import hashlib,json,random,subprocess,time
ROOT=Path(__file__).resolve().parents[2];OUT=Path(__file__).resolve().parent
pairs=[]
for unscaled in ['0','1','-1','15','-150','12300','123456789012345678901234567890123400']:
    for scale in [-2147483648,-2147483647,-100,-7,-6,-1,0,1,2,5,6,7,10,100,2147483647]:
        pairs.append((unscaled,scale))
rng=random.Random(3391107)
for length in [2,3,8,16,32,64,128,256,1024]:
    for sign in ['', '-']:
        value=sign+str(rng.randrange(1,10))+''.join(str(rng.randrange(10)) for _ in range(length-1))
        for scale in [length-1,length+5,length+6]: pairs.append((value,scale))
source='(assert (= "1.12.6" (clojure-version)))\n'
for value,scale in pairs:
    source+='(let [x (java.math.BigDecimal. (java.math.BigInteger. '+json.dumps(value)+') (int '+str(scale)+'))] (println (pr-str (str x))) (println (pr-str (Long/toHexString (Double/doubleToRawLongBits (.doubleValue x))))))\n'
(OUT/'oracle.clj').write_text(source)
prior=json.loads((ROOT/'results/m12-wide-wit/oracle-command.json').read_text())
command=prior['command'][:-1]+[str(OUT/'oracle.clj')];start=time.time()
r=subprocess.run(command,cwd=ROOT,text=True,capture_output=True)
(OUT/'oracle.log').write_text(r.stdout+r.stderr)
(OUT/'oracle-command.json').write_text(json.dumps({'command':command,'exit_code':r.returncode,
    'seconds':time.time()-start,'source_sha256':hashlib.sha256(source.encode()).hexdigest(),
    'jars':{p:hashlib.sha256(Path(p).read_bytes()).hexdigest() for p in prior['jars']}},indent=2)+'\n')
assert r.returncode==0,r.stderr
values=[json.loads(line) for line in r.stdout.splitlines()];assert len(values)==len(pairs)*2
(OUT/'format-cases.json').write_text(json.dumps([{'unscaled':v,'scale':s,'expected':values[i*2],'double_bits':values[i*2+1].zfill(16)}
    for i,(v,s) in enumerate(pairs)],indent=2)+'\n')
print(len(pairs),'fresh primary format and binary64 observations')
