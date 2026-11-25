"""Observe one exact published CI run; preserve failed logs, never rerun jobs."""
from pathlib import Path
import gzip,json,os,re,signal,subprocess,time
signal.signal(signal.SIGHUP,signal.SIG_IGN)
base=Path(__file__).resolve().parent;out=base/'pr516-ci-133b';out.mkdir(exist_ok=False)
statefile=base/'pr516-ci-133b-state.json';assert not statefile.exists()
runid=45586341822;head='133b7024762d5006d4c784416e66ecef5a73f6b8'
state={'run_id':runid,'head':head,'pid':os.getpid(),'scope':os.environ['SUSS_DURABLE_SCOPE'],'phase':'observing-exact-head-ci','complete':False,'saved_failed_logs':{},'poll_errors':[]}
assert state['scope'] in Path('/proc/self/cgroup').read_text()
def save():
 p=statefile.with_suffix('.tmp');p.write_text(json.dumps(state,indent=2)+'\n');p.replace(statefile)
def poll():
 p=subprocess.run(['gh','run','view',str(runid),'--repo','bobby/suss','--json','headSha,status,conclusion,jobs'],capture_output=True,text=True)
 if p.returncode:raise RuntimeError(p.stderr[-1000:])
 return json.loads(p.stdout)
save()
while True:
 try:
  run=poll();assert run['headSha']==head
  (out/'last-run.json').write_text(json.dumps(run,indent=2)+'\n')
  state.update(last_poll=time.time(),status=run['status'],conclusion=run['conclusion'],jobs=[{k:j.get(k) for k in ['databaseId','name','status','conclusion','startedAt','completedAt']} for j in run['jobs']])
  for job in run['jobs']:
   if job['status']!='completed' or job['conclusion'] in ['success','skipped','neutral','']:continue
   identity=str(job['databaseId'])
   if identity in state['saved_failed_logs']:continue
   log=subprocess.run(['gh','api','--allow-escape-sequences',f'repos/bobby/suss/actions/jobs/{identity}/logs'],capture_output=True)
   if log.returncode:
    state.setdefault('log_fetch_errors',{})[identity]=log.stderr.decode(errors='replace')[-600:];continue
   dest=out/f'job-{identity}.log.gz';dest.write_bytes(gzip.compress(log.stdout,mtime=0))
   plain=re.sub(r'\x1b\[[0-?]*[ -/]*[@-~]','',log.stdout.decode(errors='replace'));lines=plain.splitlines();findings=[]
   for i,line in enumerate(lines):
    if 'panicked at' in line or 'error[E' in line or 'test result: FAILED' in line or '##[error]' in line:findings.extend(lines[max(0,i-2):i+7])
   summary=out/f'job-{identity}-failure-excerpts.txt';summary.write_text('\n'.join(findings)+'\n')
   state['saved_failed_logs'][identity]={'name':job['name'],'conclusion':job['conclusion'],'raw_gzip':str(dest),'excerpts':str(summary)}
   print(json.dumps({'failed_job':job['name'],'conclusion':job['conclusion'],'log':str(dest)}),flush=True)
  if run['status']=='completed':
   byname={j['name']:j for j in run['jobs']};expected=['foundation',*[f'integration ({i})' for i in range(8)],'test']
   state.update(complete=True,phase='exact-head-ci-terminal',all_required_success=run['conclusion']=='success' and all(n in byname and byname[n]['conclusion']=='success' for n in expected));save();print(json.dumps(state,indent=2),flush=True);break
  save()
 except Exception as error:
  state['poll_errors'].append({'at':time.time(),'error':repr(error)});state['poll_errors']=state['poll_errors'][-10:];save()
 time.sleep(45)
