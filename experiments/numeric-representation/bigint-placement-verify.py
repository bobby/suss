"""Validate every candidate's complete limbs, normalization and sink checksum."""
from pathlib import Path
import json,os,subprocess,random,hashlib,time
root=Path(__file__).resolve().parents[2];outer=root.parents[2];out=root/'results/m12-bigint-placement';work=root/'target/probe/bigint-placement'
binary=Path(os.environ.get('SUSS_BIGINT_PROBE',outer/'target/probe/m12-bigint-target/debug/examples/bigint_placement'));module=work/'merged.wasm'
rng=random.Random(338);cases=[([],[]),([0],[0]),([0xffffffff],[1]),([0xffffffff]*4,[0xffffffff]*4),([0,0],[0]),([],[7,0]),([7,0],[])]
from bigint_placement_data import SHAPES
for la,lb in SHAPES+[(1,4),(4,1),(3,2),(32,8)]:
 for _ in range(3):cases.append(([rng.getrandbits(32) for _ in range(la)],[rng.getrandbits(32) for _ in range(lb)]))
encode=lambda v:''.join(f'{n:08x}' for n in v)
number=lambda v:sum(x << (32*i) for i,x in enumerate(v))
records=[]
for collector in ['auto','null']:
 cmd=[str(binary),str(module),collector,'prepare'];r=subprocess.run(cmd,capture_output=True,text=True);assert r.returncode==0,r.stderr
 records.append({'command':cmd,'exit_code':r.returncode,'stdout':r.stdout,'stderr':r.stderr})
 for a,b in cases:
  cmd=[str(binary),str(module),collector,'verify',encode(a),encode(b)];start=time.time();r=subprocess.run(cmd,capture_output=True,text=True);row={'command':cmd,'exit_code':r.returncode,'seconds':time.time()-start,'stdout':r.stdout,'stderr':r.stderr};records.append(row)
  (out/'verification.json').write_text(json.dumps({'compiler_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'module_sha256':hashlib.sha256(module.read_bytes()).hexdigest(),'cases':len(cases),'records':records,'complete':False},indent=2)+'\n')
  assert r.returncode==0,r.stderr
  lines=r.stdout.splitlines();assert len(lines)==6,r.stdout
  for line in lines:
   name,length,text,checksum=line.split();length=int(length);actual=[] if text=='-' else [int(text[i:i+8],16) for i in range(0,len(text),8)]
   expected=number(a)+number(b) if name.endswith('add') else number(a)*number(b)
   cap=max(len(a),len(b))+1 if name.endswith('add') else len(a)+len(b)
   assert len(actual)==cap,(a,b,line)
   assert number(actual)==expected,(a,b,line,expected)
   assert length==(expected.bit_length()+31)//32,(a,b,line)
   assert int(checksum)==(expected&0xffffffff)+length,(a,b,line)
print(f'{len(cases)*6*2} complete magnitude results match independent integer arithmetic',flush=True)
(out/'cases.json').write_text(json.dumps([{'a':a,'b':b,'add_hex':format(number(a)+number(b),'x'),'mul_hex':format(number(a)*number(b),'x')} for a,b in cases],indent=2)+'\n')

record={'compiler_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'module_sha256':hashlib.sha256(module.read_bytes()).hexdigest(),'cases':len(cases),'records':records,'complete':True}
(out/'verification.json').write_text(json.dumps(record,indent=2)+'\n')
