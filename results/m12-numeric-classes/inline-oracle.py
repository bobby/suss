from pathlib import Path
import hashlib, json, subprocess

OUT=Path(__file__).resolve().parent
jars=[Path('/home/bobby/.m2/repository')/p for p in [
 'org/clojure/clojure/1.12.6/clojure-1.12.6.jar',
 'org/clojure/spec.alpha/0.5.238/spec.alpha-0.5.238.jar',
 'org/clojure/core.specs.alpha/0.4.74/core.specs.alpha-0.4.74.jar']]
specs=[
 ('equal-bigint','1','(= x 1N)'),
 ('hash-bigint','1','(= (hash x) (hash 1N))'),
 ('less-than-ratio','1','(< x 3/2)'),
 ('numeric-equal-normalized-ratio','2','(== x 4/2)'),
 ('promoting-overflow','9223372036854775807',"(+' x 1)"),
 ('negative-promoting-overflow','-9223372036854775808',"(-' x 1)"),
 ('identity-separate-numeric-categories','1','(identical? x 1N)'),
 ('bigint-bitwise-rejection','1','(try (bit-and x 1N) (catch Throwable e :invalid-integer))'),
 ('ratio-bitwise-rejection','1','(try (bit-or x 1/2) (catch Throwable e :invalid-integer))'),
 ('wide-exact-compare','9007199254740993','(> x 9007199254740992N)'),
 ('negative-ratio-compare','-1','(> x -3/2)'),
 ('checked-overflow-control','9223372036854775807','(try (+ x 1) (catch Throwable e :long-overflow))'),
]
rows=[]
for label,argument,body in specs:
 for route in ['raw','boxed']:
  parameter='^long x' if route=='raw' else 'x'
  function=f'(fn [{parameter}] {body})'
  source=f'({function} {argument if route=="raw" else "(identity "+argument+")"})'
  rows.append({'label':label,'route':route,'function':function.replace('(catch Throwable','(catch :default'),'source':source.replace('(catch Throwable','(catch :default'),'jvm_source':source})
program='\n'.join('(prn '+row['jvm_source']+')' for row in rows)
command=['java','-cp',':'.join(map(str,jars)),'clojure.main','-e',program]
p=subprocess.run(command,text=True,capture_output=True)
(OUT/'inline-oracle.json').write_text(json.dumps({'command':command,'jars':{str(p):hashlib.sha256(p.read_bytes()).hexdigest() for p in jars},'source':program,'exit_code':p.returncode,'stdout':p.stdout,'stderr':p.stderr},indent=2)+'\n')
assert p.returncode==0,p.stderr
values=p.stdout.splitlines();assert len(values)==len(rows),p.stdout
for row,value in zip(rows,values): row['expected']=value
(OUT/'inline-cases.json').write_text(json.dumps(rows,indent=2)+'\n')
print('fresh JVM raw/boxed cases',len(rows),values)
