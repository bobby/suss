from pathlib import Path
import hashlib,json,os,random,subprocess,time
ROOT=Path(__file__).resolve().parents[2];OUT=Path(__file__).resolve().parent
HOST=ROOT.parent/'m12-helper-target/debug/examples/bigint_runtime';CACHE=ROOT/'target/probe/m12-production-multiply-refined'
prior=json.loads((OUT/'profile.json').read_text());assert prior['complete']
record={'source_profile':'profile.json','host_sha256':prior['host_sha256'],'modules':prior['modules'],'commands':[],'rows':[],'complete':False};rng=random.Random(338073)
shapes=[(2,7),(7,2),(2,8),(8,2),(2,9),(9,2),(2,10),(10,2),(1,8),(8,1),(1,128),(128,1),(1,1024),(1024,1),(3,4),(4,3)]
cases=[]
for la,lb in shapes:
 a=[rng.getrandbits(32) for _ in range(la)];b=[rng.getrandbits(32) for _ in range(lb)];a[-1]|=1<<31;b[-1]|=1<<31
 v=sum(x<<(32*i) for i,x in enumerate(a))*sum(x<<(32*i) for i,x in enumerate(b));product=[(v>>(32*i))&0xffffffff for i in range((v.bit_length()+31)//32)];cases.append({'a':a,'b':b,'product':product})
fixture=OUT/'boundary-cases.json';fixture.write_text(json.dumps(cases,indent=2)+'\n');record['cases_sha256']=hashlib.sha256(fixture.read_bytes()).hexdigest()
def save():(OUT/'boundary.json').write_text(json.dumps(record,indent=2)+'\n')
def run(index,candidate,count):
 assert hashlib.sha256(HOST.read_bytes()).hexdigest()==record['host_sha256']
 assert all(hashlib.sha256((ROOT/p).read_bytes()).hexdigest()==h for p,h in record['modules'].items())
 command=['perf','stat','-x,','-e','instructions:u','--',str(HOST),'run',str(CACHE),'auto',str(fixture),str(index),candidate,str(count)]
 start=time.time();r=subprocess.run(['systemd-run','--user','--scope','-q','-p','MemoryMax=3G','--',*command],cwd=ROOT,capture_output=True,text=True,env=os.environ|{'XDG_RUNTIME_DIR':'/run/user/1000'})
 record['commands'].append({'command':command,'exit_code':r.returncode,'seconds':time.time()-start,'stdout':r.stdout,'stderr':r.stderr});save();assert r.returncode==0,r.stderr
 values=[int(line.split(',')[0]) for line in r.stderr.splitlines() if ',instructions:u,' in line and line.split(',')[0].isdigit()];assert len(values)==1;return values[0]
save()
for index,(a,b) in enumerate(shapes):
 n=max(1000,min(100000,100000000//(66*a*b+70*(a+b)+300)))
 for candidate in ['gc','helper','dispatch']:
  slopes=[]
  for repeat in range(2):slopes.append((run(index,candidate,2*n)-run(index,candidate,n))/n)
  record['rows'].append({'a':a,'b':b,'candidate':candidate,'n':n,'slopes':slopes});save()
 print(a,b,flush=True)
record['complete']=True;save()
