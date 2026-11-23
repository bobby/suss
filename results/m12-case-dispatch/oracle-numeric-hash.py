"""Pinned JVM Java hashCode controls for case's numeric leaf dispatch."""
from pathlib import Path
import hashlib,json,subprocess,time
ROOT=Path(__file__).resolve().parents[2];OUT=Path(__file__).resolve().parent
values=[]
integers=[-9223372036854775808,-4294967297,-4294967296,-2147483649,-2147483648,-1073741825,-1,0,1,1073741823,1073741824,2147483647,2147483648,4294967295,4294967296,9223372036854775807]
for n in integers: values.extend([str(n),str(n)+'N','(biginteger '+str(n)+')'])
for n in [-(2**193+2**65+1),-(2**96-1),-(2**64),2**63,2**64,2**64+2**32+1,2**127-1,2**193+2**65+1]:
 values.extend([str(n)+'N','(biginteger '+str(n)+'N)'])
values.extend(['0.0','-0.0','1.0','-1.0','0.5','-0.5','1.25','3.141592653589793','4.9E-324','-4.9E-324','1.7976931348623157E308','##NaN','(/ 0.0 0.0)','##Inf','##-Inf'])
for n in ['0','1','-1','123456789012345678901234567890','-123456789012345678901234567890']:
 for suffix in ['M','.0M','.00M','E+7M','E-8M']: values.append(n+suffix)
values.extend(['0E+2147483647M','1E+2147483647M','1E-2147483647M','-1E-2147483647M','1/2','-1/2','2/3','123456789012345678901234567890/7','-123456789012345678901234567890/7','1/123456789012345678901234567891'])
prior=json.loads((ROOT/'results/m12-wide-wit/oracle-command.json').read_text());script=OUT/'numeric-hash-oracle.clj';source='(assert (= "1.12.6" (clojure-version)))\n'+''.join('(println (clojure.lang.Util/hash '+v+'))\n' for v in values);script.write_text(source)
command=prior['command'][:-1]+[str(script)];t=time.time();r=subprocess.run(command,cwd=ROOT,text=True,capture_output=True);(OUT/'numeric-hash-oracle.log').write_text(r.stdout+r.stderr)
(OUT/'numeric-hash-oracle-command.json').write_text(json.dumps({'command':command,'exit_code':r.returncode,'seconds':time.time()-t,'source_sha256':hashlib.sha256(source.encode()).hexdigest(),'jars':{p:hashlib.sha256(Path(p).read_bytes()).hexdigest() for p in prior['jars']}},indent=2)+'\n');assert r.returncode==0,r.stderr
expected=[str(int(s)) for s in r.stdout.splitlines()];assert len(expected)==len(values)
cases=[['(suss.bootstrap/case-number-hash '+v+')',e] for v,e in zip(values,expected)]
(OUT/'numeric-hash-cases.json').write_text(json.dumps(cases,indent=2)+'\n');(OUT/'numeric-hash-primary-mapping.json').write_text(json.dumps([{'jvm':'(clojure.lang.Util/hash '+v+')','suss':s,'expected':e} for v,(s,e) in zip(values,cases)],indent=2)+'\n');print(len(cases),'fresh primary numeric case-hash controls')
