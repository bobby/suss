"""Instruction slopes and null-GC magnitude-array bytes, following bench.py."""
from pathlib import Path
import subprocess,json,os,time,hashlib,random
root=Path(__file__).resolve().parents[2];outer=root.parents[2];out=root/'results/m12-bigint-placement';work=root/'target/probe/bigint-placement';binary=Path(os.environ.get('SUSS_BIGINT_PROBE',outer/'target/probe/m12-bigint-target/debug/examples/bigint_placement'));module=work/'merged.wasm'
assert json.loads((out/'oracle-command.json').read_text())['exit_code']==0
verification=json.loads((out/'verification.json').read_text());assert verification['complete'] and len(verification['records'])==2+2*verification['cases'] and all(row['exit_code']==0 for row in verification['records']);assert verification['module_sha256']==hashlib.sha256(module.read_bytes()).hexdigest()
encode=lambda v:''.join(f'{x:08x}' for x in v)
number=lambda v:sum(x << (32*i) for i,x in enumerate(v))
record={'engine':'workspace Wasmtime 51.0.0-rc.1','module_sha256':hashlib.sha256(module.read_bytes()).hexdigest(),'host_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'profiles':[],'allocation':[]};rng=random.Random(33801)
def checkpoint(): (out/'measurement.json').write_text(json.dumps(record,indent=2)+'\n')
def command(collector,a,b,name,n):return [str(binary),str(module),collector,'run',encode(a),encode(b),name,str(n)]
from bigint_placement_data import SHAPES
shapes=SHAPES
for la,lb in shapes:
 width=max(la,lb)
 a=[rng.getrandbits(32) for _ in range(la)];b=[rng.getrandbits(32) for _ in range(lb)];a[-1]|=0x80000000;b[-1]|=0x80000000
 for operation in ['add','mul']:
  n=max(5000,1000000//width) if operation=='add' else max(10,500000//(la*lb))
  expected=number(a)+number(b) if operation=='add' else number(a)*number(b);used=(expected.bit_length()+31)//32;scalar=(expected&0xffffffff)+used
  row={'limbs_a':la,'limbs_b':lb,'operation':operation,'count':n,'a':a,'b':b,'cases':{}};record['profiles'].append(row)
  for name in ['control','linear_control','gc_'+operation,'bridge_'+operation,'linear_'+operation]:
   expected_scalar=a[0]+len(a) if name in ['control','linear_control'] else scalar
   base=[]
   sample_count=max(n,1000000) if name in ['control','linear_control'] else n
   # Repeated slope pairs retain the spread rather than reporting false precision.
   for repeat in range(2):
    pair=[]
    for count in [sample_count,2*sample_count]:
     cmd=['perf','stat','-x,','-e','instructions:u','--',*command('auto',a,b,name,count)];r=subprocess.run(cmd,capture_output=True,text=True)
     item={'command':cmd,'exit_code':r.returncode,'stdout':r.stdout,'stderr':r.stderr};pair.append(item)
     assert r.returncode==0,(name,width,r.stderr)
     assert int(r.stdout)==count*expected_scalar,(name,width,count,r.stdout,expected_scalar)
     counts=[int(line.split(',')[0]) for line in r.stderr.splitlines() if ',instructions:u,' in line];assert len(counts)==1,r.stderr;item['instructions']=counts[0]
    base.append({'samples':pair,'instructions_per_operation':(pair[1]['instructions']-pair[0]['instructions'])/sample_count,'count':sample_count})
   row['cases'][name]=base;checkpoint()
  print(operation,(la,lb),{k:round(v[0]['instructions_per_operation'],2) for k,v in row['cases'].items()},flush=True)
# One persistent result is a GC array with capacity max(la,lb)+1 or la+lb.
# Measure its allocation directly; kernels allocate that same array once/op.
# This avoids billions of multiplies merely to amplify an identical byte count.
for capacity in [2,3,5,9,16,33,64,129,256,513,1024]:
 n=max(30000,(64<<20)//(capacity*4+32));a=[1]*capacity;b=[];row={'capacity':capacity,'count':n,'samples':[]};record['allocation'].append(row)
 for count in [n,2*n,n,2*n]:
  cmd=command('null',a,b,'allocate',count)
  with (work/'rss.stdout').open('w') as stdout:
   process=subprocess.Popen(cmd,stdout=stdout,stderr=subprocess.PIPE,env=os.environ|{'SUSS_BIGINT_RSS':'1'});_,status,usage=os.wait4(process.pid,0);stderr=process.stderr.read().decode();process.returncode=os.waitstatus_to_exitcode(status)
  text=(work/'rss.stdout').read_text();assert process.returncode==0,stderr;assert int(text)==count*(capacity+1),(capacity,text,count)
  peak=[int(line.split('=')[1]) for line in stderr.splitlines() if line.startswith('peak-bytes=')];assert len(peak)==1,stderr
  row['samples'].append({'command':cmd,'exit_code':process.returncode,'stdout':text,'stderr':stderr,'peak_bytes':peak[0],'inherited_rusage_peak_bytes':usage.ru_maxrss*1024})
 row['bytes_per_operation']=[(row['samples'][i+1]['peak_bytes']-row['samples'][i]['peak_bytes'])/n for i in [0,2]];checkpoint();print('array',capacity,row['bytes_per_operation'],flush=True)
