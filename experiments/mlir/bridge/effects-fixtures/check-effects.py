#!/usr/bin/env python3
# Original isolated evaluation regressions; repository MIT/Apache-2.0 terms.
import json, subprocess, sys, tempfile
from pathlib import Path
P=Path(__file__).parent
exe=sys.argv[1]
def run(text):
 with tempfile.TemporaryDirectory() as d:
  source=Path(d)/'input.mlir';output=Path(d)/'output.json';source.write_text(text);output.write_text('sentinel')
  r=subprocess.run([exe,'--effects',str(source),'--export-graph='+str(output)],capture_output=True,text=True,timeout=15)
  return r,output.read_text()
def check(g):
 assert set(g)=={'schema','cells','entry'} and g['schema']=='suss.mlir.effects.v1'
 def region(f,seen=None,allocated=None):
  seen=set() if seen is None else seen.copy();allocated=set() if allocated is None else allocated
  for p in f['parameters']: assert p['id'] not in allocated;allocated.add(p['id']);seen.add(p['id'])
  for op in f['operations']:
   assert op['id'] not in allocated;allocated.add(op['id'])
   for k in ['lhs','rhs','condition','callee','value','body','handler','cleanup']:
    if k in op and isinstance(op[k],int): assert op[k] in seen,(k,op)
   for v in op.get('captures',[])+op.get('arguments',[]):assert v in seen
   if op['op']=='closure':region(op['function'])
   if op['op']=='if':
    region(op['consequent'],seen,allocated);region(op['alternative'],seen,allocated)
   seen.add(op['id'])
  assert f['terminator']['value'] in seen
 region(g['entry'])
positives=list(P.glob('*.mlir'))
for f in positives:
 r,out=run(f.read_text());assert r.returncode==0,(f,r.stderr);g=json.loads(out);check(g);f.with_suffix('.json').write_text(json.dumps(g,indent=2)+'\n')
base=(P/'success.mlir').read_text()
mutations={
 'unknown-module-attr':base.replace('suss.cells =','unknown = 1 : i64, suss.cells ='),
 'unknown-op-attr':base.replace('value = 10.0 : f64','value = 10.0 : f64, unknown = true',1),
 'unknown-op':base.replace('"suss.equal"','"suss.unknown"',1),
 'undeclared-cell':base.replace('name="journal"} :','name="missing"} :',1),
 'duplicate-cell':base.replace('name="count", initial_bits','name="journal", initial_bits',1),
 'invalid-bits':base.replace('0000000000000000','000000000000000G',1),
 'wrong-capture-type':base.replace('[],','[i64],',1) if '[],' in base else base.replace('-> !suss.value, []>','-> !suss.value, [i64]>',1),
 'multi-result':base.replace('-> !suss.value, []>','-> (!suss.value, !suss.value), []>',1),
 'wrong-argument-type':base.replace('(!suss.value) -> !suss.value','(f64) -> !suss.value',1),
 'wrong-try-arity':base.replace('%body,%handler,%cleanup','%body,%cleanup,%handler',1),
 'wrong-condition':base.replace('"suss.if"(%eq)','"suss.if"(%test)',1).replace('}) : (i1) -> !suss.value','}) : (!suss.value) -> !suss.value',1),
 'empty-arm':base.replace('"suss.yield"(%yes) : (f64) -> ()','',1),
 'escaping-arm':base.replace('"suss.effect_return"(%choice)','"suss.effect_return"(%a2)',1),
 'wrong-return-parent':base.replace('"suss.yield"','"suss.effect_return"',1),
 'implicit-capture':base.replace('"suss.effect_return"(%payload)','"suss.effect_return"(%result)',1),
 'source-attr':base.replace('suss.cells =','suss.source_analysis = "invalid", suss.cells ='),
}
throw=(P/'handled-throw.mlir').read_text();mutations['dynamic-throw']=throw.replace('"suss.throw"(%bad) : (f64)','"suss.throw"(%choice) : (!suss.value)',1)
(P/'negative').mkdir(exist_ok=True)
for label,text in mutations.items():
 (P/'negative'/(label+'.mlir')).write_text(text)
 assert text!=base or label=='dynamic-throw',label
 r,out=run(text);assert r.returncode!=0 and out=='sentinel',(label,r.stderr,out)
if len(sys.argv)>2:
 for f in positives:
  with tempfile.TemporaryDirectory() as d:
   printed=Path(d)/'printed.mlir'
   r=subprocess.run([sys.argv[2],str(f),'-o',str(printed)],capture_output=True,text=True,timeout=15)
   assert r.returncode==0,(f,r.stderr)
   r,out=run(printed.read_text());assert r.returncode==0,(f,r.stderr)
   assert json.loads(out)==json.loads(f.with_suffix('.json').read_text()),f
 print('PASS: registered effects print/parse/export roundtrip')
print(f'PASS: {len(positives)} effects exports, logical dominance/IDs, {len(mutations)} rejection probes; Wasmtime pending')
