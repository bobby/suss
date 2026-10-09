#!/usr/bin/env python3
"""Roundtrip genuine source-aware HIR through strict MLIR storage; no lowering claim."""
import copy,json,re,subprocess,sys,tempfile
from pathlib import Path
SOURCE='(let [^number x 7 f (fn [y] [x y false nil])] (let [x 9] (f x)))'
def run(args, text=None):
 r=subprocess.run(args,input=text,capture_output=True,text=True,timeout=30)
 assert r.returncode==0, r.stderr.splitlines()[0] if r.stderr else str(r.returncode)
 return r.stdout
def decode(output):
 raw=re.search(r'suss.source_analysis\s*=\s*"((?:\\.|[^"\\])*)"',output)[1]
 data=bytearray();i=0
 while i<len(raw):
  if raw[i]=='\\':data.append(int(raw[i+1:i+3],16));i+=3
  else:data.extend(raw[i].encode());i+=1
 return json.loads(data)
with tempfile.TemporaryDirectory(prefix='suss-native-facts-') as directory:
 source=Path(directory)/'source.sus';source.write_text(SOURCE)
 facts=json.loads(run([sys.argv[1],'--analyze-source',str(source)]))
 encoded=''.join('\\'+format(b,'02X') for b in json.dumps(facts,separators=(',',':')).encode())
 first=run([sys.argv[2]],'module attributes {suss.source_analysis = "'+encoded+'"} {}\n')
 second=run([sys.argv[2]],first)
 assert first==second and decode(first)==facts and decode(second)==facts
 # Consistently change an outer declaration's physical ID only inside each
 # callable body. Local visibility remains internally consistent; the method
 # entry snapshot must still reject an outer remap (only params are remapped).
 changed=copy.deepcopy(facts)
 def remap(value):
  if isinstance(value,dict):
   if value.get('bindingId')=='binding:0' and 'hirBindingId' in value: value['hirBindingId']=100
   for child in value.values():remap(child)
  elif isinstance(value,list):
   for child in value:remap(child)
 def bodies(value):
  if isinstance(value,dict):
   if isinstance(value.get('callable'),dict): remap(value['callable']['body'])
   for child in value.values():bodies(child)
  elif isinstance(value,list):
   for child in value:bodies(child)
 bodies(changed)
 assert changed!=facts
 encoded=''.join('\\'+format(b,'02X') for b in json.dumps(changed,separators=(',',':')).encode())
 rejected=subprocess.run([sys.argv[2]],input='module attributes {suss.source_analysis = "'+encoded+'"} {}\n',capture_output=True,text=True,timeout=30)
 assert rejected.returncode!=0 and 'callable body scope disagrees with declarations' in rejected.stderr.splitlines()[0],rejected.stderr.splitlines()[0] if rejected.stderr else rejected.returncode

print('PASS complete genuine native source facts survive strict MLIR print/parse twice')
