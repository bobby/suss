import hashlib,json,os,statistics,subprocess,tempfile,time
from pathlib import Path
root=Path(__file__).resolve().parent
exe=root.parent/'m12-box-optimized-suss'
base_env=dict(os.environ,SUSS_BUILD_DIR='/home/bobby/.codex-workspaces/worktrees/ios-cd7ccbc9-1c87-43fe-95e7-2b6b71168849/suss/target/suss')
names=['double-raw','double-boxed','long-raw','long-boxed']
result={'compiler_source':'cda523ffebcf546e7033a3adac1c54d401c37f91 plus retained selective-cache source patch','executable_sha256':hashlib.sha256(exe.read_bytes()).hexdigest(),'component_sha256':hashlib.sha256((root/'bench.wasm').read_bytes()).hexdigest(),'allocation_method':'bench.py null-collector peak-RSS slope; numeric bytes are process-RSS estimates, not exact object counts','rows':[]}

def run(name,n,kind):
    command=[str(exe),'run','bench.wasm','--invoke',name,'--',str(n)]
    env=dict(base_env)
    if kind=='rss': env['SUSS_GC']='null'
    if kind=='instructions': command=['perf','stat','-x,','-e','instructions:u','--',*command]
    with tempfile.TemporaryFile() as stdout,tempfile.TemporaryFile() as stderr:
        start=time.perf_counter()
        process=subprocess.Popen(command,cwd=root,env=env,stdout=stdout,stderr=stderr)
        _,status,usage=os.wait4(process.pid,0)
        process.returncode=os.waitstatus_to_exitcode(status)
        elapsed=time.perf_counter()-start
        stdout.seek(0); output=stdout.read().decode().strip()
        stderr.seek(0); errors=stderr.read().decode()
    if status or output!=str(n): raise RuntimeError((name,n,kind,status,output,errors))
    row={'n':n,'seconds':elapsed,'peak_rss_bytes':usage.ru_maxrss*1024}
    if kind=='instructions':
        counts=[int(line.split(',')[0]) for line in errors.splitlines() if 'instructions' in line and line.split(',')[0].isdigit()]
        if len(counts)!=1: raise RuntimeError(errors)
        row['instructions']=counts[0]
    return row

for name in names:
    run(name,1,'rss');run(name,1,'instructions')
    rss=[run(name,n,'rss') for n in [2000000,4000000] for _ in range(3)]
    instructions=[run(name,n,'instructions') for n in [100000,200000] for _ in range(3)]
    row={'name':name,'rss':rss,'instruction_samples':instructions,
         'bytes_per_iteration':(statistics.median(r['peak_rss_bytes'] for r in rss if r['n']==4000000)-statistics.median(r['peak_rss_bytes'] for r in rss if r['n']==2000000))/2000000,
         'instructions_per_iteration':(statistics.median(r['instructions'] for r in instructions if r['n']==200000)-statistics.median(r['instructions'] for r in instructions if r['n']==100000))/100000}
    result['rows'].append(row)
    (root/'measurements.json').write_text(json.dumps(result,indent=2)+'\n')
    print(json.dumps({k:v for k,v in row.items() if k not in ['rss','instruction_samples']}),flush=True)
