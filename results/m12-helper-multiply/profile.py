"""Measure complete production wrappers with instruction and allocation columns."""
from pathlib import Path
import csv,hashlib,json,os,random,re,subprocess,time
ROOT=Path(__file__).resolve().parents[2];OUT=Path(__file__).resolve().parent
HOST=ROOT.parent/'m12-helper-target/debug/examples/bigint_runtime'
CACHE=ROOT/'target/probe/m12-production-multiply';CACHE.mkdir(parents=True,exist_ok=True)
ENV=os.environ|{'XDG_RUNTIME_DIR':'/run/user/1000'}
base=json.loads((OUT/'inputs.json').read_text())
base['crates/suss-compile/examples/bigint_runtime.rs']=hashlib.sha256((ROOT/'crates/suss-compile/examples/bigint_runtime.rs').read_bytes()).hexdigest()
record={'inputs':base,'host_sha256':hashlib.sha256(HOST.read_bytes()).hexdigest(),'commands':[],'verification':[],'instructions':[],'bytes':[],'complete':False}
def save():(OUT/'profile.json').write_text(json.dumps(record,indent=2)+'\n')
def run(command):
 assert hashlib.sha256(HOST.read_bytes()).hexdigest()==record['host_sha256']
 start=time.time();r=subprocess.run(['systemd-run','--user','--scope','-q','-p','MemoryMax=3G','--',*map(str,command)],cwd=ROOT,env=ENV,capture_output=True,text=True)
 row={'command':list(map(str,command)),'exit_code':r.returncode,'seconds':time.time()-start,'stdout':r.stdout,'stderr':r.stderr};record['commands'].append(row);save()
 assert r.returncode==0,row
 return row
for collector in ['auto','null']:run([HOST,'prepare',CACHE,collector])
record['modules']={str(p.relative_to(ROOT)):hashlib.sha256(p.read_bytes()).hexdigest() for p in CACHE.iterdir() if p.suffix in ['.wasm','.cwasm']};save()
cases=json.loads((OUT/'cases.json').read_text());rng=random.Random(338072)
for la,lb in [(2,3),(2,4),(2,5),(2,6),(3,4),(3,5),(3,6),(4,3)]:
 a=[rng.getrandbits(32) for _ in range(la)];b=[rng.getrandbits(32) for _ in range(lb)];a[-1]|=1<<31;b[-1]|=1<<31
 n=sum(x<<(32*i) for i,x in enumerate(a))*sum(x<<(32*i) for i,x in enumerate(b))
 product=[(n>>(32*i))&0xffffffff for i in range((n.bit_length()+31)//32)]
 cases.append({'a':a,'b':b,'product':product})
fixture=OUT/'profile-cases.json';fixture.write_text(json.dumps(cases,indent=2)+'\n');record['cases_sha256']=hashlib.sha256(fixture.read_bytes()).hexdigest();save()
def invoke(index,candidate,count,collector='auto',perf=False):
 assert all(hashlib.sha256((ROOT/p).read_bytes()).hexdigest()==h for p,h in record['modules'].items())
 command=[HOST,'run',CACHE,collector,fixture,index,candidate,count]
 if perf:command=['perf','stat','-x,','-e','instructions:u','--',*command]
 row=run(command)
 if perf:
  counts=[int(line.split(',')[0]) for line in row['stderr'].splitlines() if ',instructions:u,' in line and line.split(',')[0].isdigit()]
  assert len(counts)==1,row
  return counts[0]
 peak=re.findall(r'peak-bytes=(\d+)',row['stderr']);assert len(peak)==1,row
 return int(peak[0])
for index,case in enumerate(cases):
 for collector in ['auto','null']:
  for candidate in ['gc','helper','dispatch','control','allocate']:
   invoke(index,candidate,1,collector)
   record['verification'].append({'index':index,'collector':collector,'candidate':candidate});save()
print('verified',len(record['verification']),'complete wrapper observations',flush=True)
for index,case in enumerate(cases):
 la,lb=len(case['a']),len(case['b']);count=max(16,min(200000,100000000//max(1,66*la*lb+70*(la+lb)+300)))
 for candidate in ['gc','helper','dispatch','control']:
  n=max(count,100000) if candidate=='control' else count
  slopes=[]
  for repeat in range(2):
   a=invoke(index,candidate,n,perf=True);b=invoke(index,candidate,2*n,perf=True);slopes.append((b-a)/n)
  record['instructions'].append({'index':index,'a':la,'b':lb,'candidate':candidate,'n':n,'slopes':slopes});save()
 print('instructions',index,la,lb,flush=True)
for index in [0,2,7,12,14]:
 case=cases[index];cap=len(case['a'])+len(case['b']);n=(128*1024*1024)//(4*cap+64);slopes=[]
 for repeat in range(2):
  a=invoke(index,'allocate',n,'null');b=invoke(index,'allocate',2*n,'null');slopes.append((b-a)/n)
 record['bytes'].append({'index':index,'capacity':cap,'n':n,'slopes':slopes});save()
 assert min(slopes)>=4*cap,('RSS slope below its own result payload',record['bytes'][-1])
assert all(hashlib.sha256((ROOT/p).read_bytes()).hexdigest()==h for p,h in base.items())
record.update(complete=True,inputs_unchanged=True);save()
with (OUT/'instructions.csv').open('w') as f:
 writer=csv.writer(f);writer.writerow(['limbs_a','limbs_b','candidate','instructions_with_control_low','instructions_with_control_high','instructions_net_low','instructions_net_high'])
 for row in record['instructions']:
  controls=next(r['slopes'] for r in record['instructions'] if r['index']==row['index'] and r['candidate']=='control')
  writer.writerow([row['a'],row['b'],row['candidate'],min(row['slopes']),max(row['slopes']),min(row['slopes'])-max(controls),max(row['slopes'])-min(controls)])
with (OUT/'bytes.csv').open('w') as f:
 writer=csv.writer(f);writer.writerow(['capacity','bytes_low','bytes_high'])
 for row in record['bytes']:writer.writerow([row['capacity'],min(row['slopes']),max(row['slopes'])])
print('terminal production wrapper measurements',flush=True)
