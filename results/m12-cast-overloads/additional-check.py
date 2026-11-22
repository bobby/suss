from pathlib import Path
import json,subprocess,os,hashlib
root=Path.cwd();p=root/'results/m12-cast-overloads';work=root/'target/probe/m12-cast-overloads-aot';work.mkdir(exist_ok=True)
outer=root.parents[2];binary=outer/'target/probe/m12-hints-target/debug/suss'
env=os.environ|{'SUSS_BUILD_DIR':str(outer/'target/suss')}
sources=(root/'results/m12-bits-casts/additional.sus').read_text()
wit=(root/'results/m12-bits-casts/additional.wit').read_text()
extra=[('(let [x (unchecked-float 2147483648.0)] [(int x) (try (int (identity x)) (catch :default e :error))])','[2147483647 :error]','[2147483647 :error]'),('[(double (float (unchecked-float ##Inf))) (try (float (identity (unchecked-float ##Inf))) (catch :default e :error)) (try ((identity float) (unchecked-float ##Inf)) (catch :default e :error))]','[##Inf :error :error]','[##Inf ##Inf ##Inf]')]
for i,(source,expected,jvm) in enumerate(extra,8):
 sources+='\n(def case'+str(i)+' (fn [] (if (= (pr-str (try '+source+' (catch :default e :error))) '+json.dumps(expected)+') 1 0)))\n'
 wit=wit[:-1]+f'export case{i}: func() -> u32;\n'+'}'
for suffix,text in [('sus',sources),('wit',wit)]:
 (work/f'additional.{suffix}').write_text(text);(p/f'additional.{suffix}').write_text(text)
cmd=[str(binary),'compile','additional.sus','-w','additional.wit','-o','current.wasm']
for i in range(10):cmd+=['--export',f'case{i}=additional/case{i}']
with (p/'additional-compile.log').open('w') as log:r=subprocess.run(cmd,cwd=work,env=env,stdout=log,stderr=subprocess.STDOUT)
assert r.returncode==0,r.returncode
observations=[]
with (p/'additional-run.log').open('w') as log:
 for i in range(10):
  cmd=[str(binary),'run','current.wasm','--invoke',f'case{i}'];r=subprocess.run(cmd,cwd=work,env=env,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True)
  observations.append({'export':f'case{i}','exit_code':r.returncode,'stdout':r.stdout.strip(),'stderr':r.stderr});log.write(json.dumps(observations[-1])+'\n')
record={'compiler_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'component_sha256':hashlib.sha256((work/'current.wasm').read_bytes()).hexdigest(),'before_receipt':'../m12-bits-casts/additional-observations.json','extra_cases':[dict(source=s,suss_expected=v,jvm_expected=j) for s,v,j in extra],'observations':observations}
(p/'additional-observations.json').write_text(json.dumps(record,indent=2)+'\n')
assert all(o['exit_code']==0 and o['stdout']=='1' for o in observations)
print('actual additional AOT',len(observations),'passed')
