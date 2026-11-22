from pathlib import Path
import os,subprocess,json,hashlib,shutil,time,sys
root=Path.cwd();p=root/'results/m12-escape';work=root/'target/probe/m12-escape-bench';work.mkdir(exist_ok=True);outer=root.parents[2]
binary=outer/'target/probe/m12-hints-target/debug/suss';frozen=binary
expressions={
 'control':'(+ x y)',
 'vector':'(let [[a b] [x y]] (+ a b))',
 'entry':'(let [[a b] (MapEntry. x y nil)] (+ a b))',
 'closure':'(let [f (fn [] (+ x y))] (f))',
}
source='(ns escapebench)\n';wit='package test:escape; world w {\n'
for name,expr in expressions.items():
 source+=f'(def case-{name} (fn [n] (let [n (long n)] (loop [i 0 acc 0.0] (if (< i n) (let [x (+ 3000000000 i) y (double i)] (recur (inc i) (unchecked-add acc (double {expr})))) acc)))))\n'
 wit+=f'export case-{name}: func(n: u32) -> f64;\n'
wit+='}\n'
for suffix,text in [('sus',source),('wit',wit)]:
 (work/f'bench.{suffix}').write_text(text);(p/f'bench.{suffix}').write_text(text)
env=os.environ|{'SUSS_BUILD_DIR':str(outer/'target/suss')}
cmd=[str(frozen),'compile','bench.sus','-w','bench.wit','-o','after.wasm']
for name in expressions:cmd+=['--export',f'case-{name}=escapebench/case-{name}']
with (p/'after-compile.log').open('w') as log:r=subprocess.run(cmd,cwd=work,env=env,stdout=log,stderr=subprocess.STDOUT)
assert r.returncode==0,r.returncode
subprocess.run([sys.executable,str(p/'source-check.py')],check=True)
record={'source_inputs':'source-inputs.json','source_base':subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),'compiler_sha256':hashlib.sha256(frozen.read_bytes()).hexdigest(),'component_sha256':hashlib.sha256((work/'after.wasm').read_bytes()).hexdigest(),'cases':{}}
perf=shutil.which('perf')
with (p/'after-runs.log').open('w') as log:
 for name in expressions:
  case={};record['cases'][name]=case
  cmd=[str(frozen),'run','after.wasm','--invoke',f'case-{name}','--','10'];r=subprocess.run(cmd,cwd=work,env=env,capture_output=True,text=True);case['value_check']={'count':10,'status':r.returncode,'stdout':r.stdout,'stderr':r.stderr};assert r.returncode==0 and float(r.stdout.strip())==30000000000+90,(name,r.stdout,r.stderr)
  # Warm the independent null-GC native cache before measuring peak RSS.
  subprocess.run(cmd,cwd=work,env=env|{'SUSS_GC':'null'},capture_output=True,check=True)
  rss=[]
  for n in [1000000,2000000,1000000,2000000]:
   cmd=[str(frozen),'run','after.wasm','--invoke',f'case-{name}','--',str(n)]
   with (p/'after-peak.stdout').open('w') as out:
    proc=subprocess.Popen(cmd,cwd=work,env=env|{'SUSS_GC':'null'},stdout=out,stderr=subprocess.PIPE)
    _,status,usage=os.wait4(proc.pid,0);stderr=proc.stderr.read().decode();proc.returncode=os.waitstatus_to_exitcode(status)
   stdout=(p/'after-peak.stdout').read_text();assert proc.returncode==0 and float(stdout)==3000000000*n+n*(n-1),(name,n,proc.returncode,stderr,stdout)
   rss.append({'count':n,'peak_bytes':usage.ru_maxrss*1024,'stdout':stdout,'stderr':stderr});log.write(json.dumps({'case':name,**rss[-1]})+'\n');log.flush()
  case['rss']=rss;case['bytes_per_iteration_samples']=[(rss[j+1]['peak_bytes']-rss[j]['peak_bytes'])/1000000 for j in [0,2]]
  if perf:
   counts=[]
   for n in [100000,200000]:
    cmd=[perf,'stat','-x,','-e','instructions:u','--',str(frozen),'run','after.wasm','--invoke',f'case-{name}','--',str(n)]
    r=subprocess.run(cmd,cwd=work,env=env,capture_output=True,text=True);assert r.returncode != 0 or float(r.stdout)==3000000000*n+n*(n-1);counts.append({'count':n,'status':r.returncode,'stdout':r.stdout,'stderr':r.stderr});log.write(json.dumps({'case':name,'perf':counts[-1]})+'\n');log.flush()
   case['perf']=counts
  (p/'after.json').write_text(json.dumps(record,indent=2)+'\n');print(name,case['bytes_per_iteration_samples'],flush=True)
(p/'after-peak.stdout').unlink()
