"""Use unchanged bench.py measurement code for bounded compiled BigInt cases."""
from pathlib import Path
import csv,hashlib,importlib.util,json,os,subprocess,sys,time
ROOT=Path(__file__).resolve().parents[2];OUT=Path(__file__).resolve().parent
BINARY=ROOT.parent/'m12-helper-target/debug/suss';BENCH=ROOT/'experiments/numeric-representation/suss-jvm/bench.py'
sys.set_int_max_str_digits(0)
def digest(path):
 h=hashlib.sha256()
 with path.open('rb') as file:
  for chunk in iter(lambda:file.read(262144),b''):h.update(chunk)
 return h.hexdigest()
record={'binary_sha256':digest(BINARY),'bench_sha256':digest(BENCH),'commands':[],'complete':False,'scope':'Suss instruction/allocation columns only; JVM timings are not measured.'}
def save():(OUT/'compiled-bench.json').write_text(json.dumps(record,indent=2)+'\n')
original_run=subprocess.run
original_popen=subprocess.Popen
original_wait4=os.wait4
peaks={}
def run(command,*args,**kwargs):
 assert digest(BINARY)==record['binary_sha256']
 start=time.time();r=original_run(command,*args,**kwargs)
 record['commands'].append({'command':list(map(str,command)),'cwd':str(kwargs.get('cwd',ROOT)),'collector':kwargs.get('env',{}).get('SUSS_GC','auto'),'exit_code':r.returncode,'seconds':time.time()-start,'stdout':r.stdout,'stderr':r.stderr});save()
 if r.returncode==0 and 'run' in command and 'bench.wasm' in command:assert r.stdout.strip()=='0',r.stdout
 return r
cases=json.loads((ROOT/'results/m12-helper-multiply/cases.json').read_text())
def integer(limbs):return sum(x<<(32*i) for i,x in enumerate(limbs))
selected=[('balanced-small',7),('balanced-large',12),('one-limb-wide',8),('two-limb-wide',10)]
setups=[];measure=[('long-control','i',None,1)];checks=[]
for label,index in selected:
 case=cases[index];a,b,p=[integer(case[key]) for key in ['a','b','product']]
 suffix=label.replace('-','_');aname='a_'+suffix;bname='b_'+suffix
 setups.extend([(aname,f'{a}N',''),(bname,f'{b}N','')])
 measure.extend([(label+'-control',aname,None,1),(label,f'(* {aname} {bname})',label+'-control',1)])
 checks.append(f'(= (pr-str (* {a}N {b}N)) {json.dumps(str(p)+"N")})')
validation='(and '+' '.join(checks)+')';(OUT/'compiled-bench-validation.sus').write_text(validation+'\n')
# Validate the same products through the compiled, unmetered component path
# used by bench.py. The earlier CLI-e default-fuel failure remains retained.
work=ROOT/'target/probe/m12-compiled-bigint-bench';work.mkdir(parents=True,exist_ok=True)
(work/'validation.sus').write_text('(ns bench.validation) (def check (fn [] '+validation+'))\n')
(work/'validation.wit').write_text('package bench:bigint-validation; world api { export check: func() -> bool; }\n')
save();r=run([str(BINARY),'--build-dir',str(work/'build-validation'),'compile','validation.sus','-w','validation.wit','--export','check=bench.validation/check','-o','validation.wasm'],cwd=work,capture_output=True,text=True)
assert r.returncode==0,r.stderr
r=run([str(BINARY),'run','validation.wasm','--invoke','check'],cwd=work,capture_output=True,text=True)
assert r.returncode==0 and r.stdout.strip()=='true',r.stderr
spec=importlib.util.spec_from_file_location('suss_numeric_bench',BENCH);bench=importlib.util.module_from_spec(spec)
argv=sys.argv;sys.argv=[str(BENCH),'10000','2'];spec.loader.exec_module(bench);sys.argv=argv
bench.SUSS=str(BINARY);bench.SETUP=setups;bench.DEFS=[];bench.CASES=measure;bench.REDUCTIONS=[];bench.PROGRAMS=[];bench.LOOPS=[];bench.WRAP={}
assert bench.PERF,'perf is required for instruction columns'
record['cases']=measure;record['settings']={name:getattr(bench,name) for name in ['N','REPS','MIN_WORK','MAX_COUNT','ALLOC_WORK','ALLOC_RESOLUTION','ALLOC_COUNTS']};save()
work=ROOT/'target/probe/m12-compiled-bigint-bench';work.mkdir(parents=True,exist_ok=True)
def popen(command,*args,**kwargs):
 if kwargs.get('env',{}).get('SUSS_GC')!='null':return original_popen(command,*args,**kwargs)
 path=work/f'peak-{len(record["commands"])}.stderr';log=path.open('w');kwargs['stderr']=log
 process=original_popen(command,*args,**kwargs);peaks[process.pid]=(list(map(str,command)),time.time(),path,log)
 return process
def wait4(pid,options):
 observed,status,usage=original_wait4(pid,options)
 if observed in peaks:
  command,start,path,log=peaks.pop(observed);log.close()
  record['commands'].append({'command':command,'collector':'null','exit_code':os.waitstatus_to_exitcode(status),'seconds':time.time()-start,'peak_bytes_wait4':usage.ru_maxrss*(1 if sys.platform=='darwin' else 1024),'stderr':path.read_text()});save()
 return observed,status,usage
bench.subprocess.run=run;bench.subprocess.Popen=popen;bench.os.wait4=wait4
try:times,instructions,allocated=bench.suss_times(str(work))
finally:subprocess.run=original_run;subprocess.Popen=original_popen;os.wait4=original_wait4
record.update(times=times,instructions=instructions,allocated=allocated,complete=True)
record['artifacts']={str(p.relative_to(ROOT)):digest(p) for p in [work/'bench.sus',work/'bench.wit',work/'bench.wasm']};save()
with (OUT/'compiled-bench.csv').open('w') as f:
 writer=csv.writer(f);writer.writerow(['case','instructions_with_control','instructions_net','bytes_measured','bytes_net_if_both_measured'])
 for name,expr,control,_ in measure:
  net=instructions[name]-(instructions[control] if control else 0)
  size=allocated[name];net_size=size-allocated[control] if control and size is not None and allocated[control] is not None else None
  writer.writerow([name,instructions[name],net,size,net_size])
print('terminal compiled bench.py columns; missing RSS slopes remain unmeasured',flush=True)
