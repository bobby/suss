"""Primary Util.hash controls for immutable Suss case literal hashing."""
from pathlib import Path
import hashlib,json,subprocess,time
ROOT=Path(__file__).resolve().parents[2];OUT=Path(__file__).resolve().parent
values=['nil','false','true','""','"a"','"Aa"','"BB"','"AaAa"','"BBBB"','"\\uD800"','"\\uDC00"','"\\uD83D\\uDE00"',"'a","'Aa","'BB","'app/name","'other/name",':a',':Aa',':BB',':app/name',':other/name','[]',"'()",'{}','#{}','[nil false true]','[1 1N 1.0 1M 1.00M]','[1N]','[1]','(list 1N)','(list 1)','[[1M] {:a #{1N 2}}]','{:a 1M :b [1N]}','#{1 2N 3.0 4M}','(sorted-map :a 1M :b [1N])','(sorted-set 1 2 3)','(with-meta [1M] {:m true})',"(with-meta 'a {:m true})",'(with-meta {:a [1M]} {:m true})','(with-meta #{1M} {:m true})']
for n in ['-1','0','1','4294967296','9223372036854775807','123456789012345678901234567890N','1/3','1.0M','1.00M','-0.0','##NaN']:
 values.extend(['['+n+']','(list '+n+')','{:a '+n+'}','#{'+n+'}'])
values.extend(['(vec (range 64))','(apply list (range 64))','(into {} (map (fn [x] [x [x 1.0M]]) (range 32)))','(set (range 64))'])
prior=json.loads((OUT/'oracle-command.json').read_text());script=OUT/'composite-hash-oracle.clj';source='(assert (= "1.12.6" (clojure-version)))\n'+''.join('(println (clojure.lang.Util/hash '+v+'))\n' for v in values);script.write_text(source)
command=prior['command'][:-1]+[str(script)];t=time.time();r=subprocess.run(command,cwd=ROOT,text=True,capture_output=True);(OUT/'composite-hash-oracle.log').write_text(r.stdout+r.stderr);(OUT/'composite-hash-oracle-command.json').write_text(json.dumps({'command':command,'exit_code':r.returncode,'seconds':time.time()-t,'source_sha256':hashlib.sha256(source.encode()).hexdigest(),'jars':{p:hashlib.sha256(Path(p).read_bytes()).hexdigest() for p in prior['jars']}},indent=2)+'\n');assert r.returncode==0,r.stderr
expected=[str(int(s)) for s in r.stdout.splitlines()];assert len(expected)==len(values)
cases=[['(suss.bootstrap/case-hash-code '+v+')',e] for v,e in zip(values,expected)]
(OUT/'composite-hash-cases.json').write_text(json.dumps(cases,indent=2)+'\n');(OUT/'composite-hash-primary-mapping.json').write_text(json.dumps([{'jvm':'(clojure.lang.Util/hash '+v+')','suss':s,'expected':e} for v,(s,e) in zip(values,cases)],indent=2)+'\n');print(len(cases),'fresh primary composite case-hash controls')
