"""Front-code actual pinned JVM name observations; preserve full raw hashes."""
from pathlib import Path
import hashlib,json,re,struct,gzip,subprocess
base=Path(__file__).resolve().parent;root=base.parents[2];out=root/'crates/suss-reader/src/pattern';out.mkdir(exist_ok=True)
receipt=json.loads((base/'command.json').read_text());assert receipt['exit_code']==0
raw=(base/'names.txt').read_bytes() if (base/'names.txt').exists() else gzip.decompress((base/'names.txt.gz').read_bytes());assert hashlib.sha256(raw).hexdigest()==receipt['names.txt_sha256'];records={line.split(b'\t')[0]:int(line.split(b'\t')[1]) for line in raw.splitlines()};names=sorted(records);assert all(name.isascii() for name in names)
block=32;data=bytearray();offsets=[]
for at in range(0,len(names),block):
 offsets.append(len(data));before=b''
 for name in names[at:at+block]:
  prefix=0
  while prefix<min(len(name),len(before)) and name[prefix]==before[prefix]:prefix+=1
  suffix=name[prefix:];assert prefix<256 and len(suffix)<256
  data.extend(bytes([prefix,len(suffix)]));data.extend(suffix);data.extend(struct.pack('<I',records[name]));before=name
offsets.append(len(data));payload=struct.pack('<II',len(names),len(offsets)-1)+b''.join(struct.pack('<I',v) for v in offsets)+data
# Independently decompress every block and require exact agreement with the observations.
found=[]
for start,end in zip(offsets,offsets[1:]):
 at=start;before=b''
 while at<end:
  prefix,length=data[at:at+2];at+=2;name=before[:prefix]+data[at:at+length];at+=length;point=struct.unpack('<I',data[at:at+4])[0];at+=4;found.append((bytes(name),point));before=name
assert found==[(name,records[name]) for name in names]
(out/'unicode-names.bin').write_bytes(payload)
properties=(base/'properties.edn').read_text();assert hashlib.sha256(properties.encode()).hexdigest()==receipt['properties.edn_sha256'];registries={}
for match in re.finditer(r':([a-z-]+) \[([^]]*)\]',properties):
 registries[match[1]]=sorted(set(json.loads('"'+m+'"') for m in re.findall(r'"((?:\\.|[^"\\])*)"',match[2])))
assert set(registries)=={'plain','plain-folded','is','is-folded','scripts','blocks','categories'}
lines=['// Generated from pinned JVM property-admission observations; no JDK code.\n']
for key,values in registries.items():
 lines.append('pub const '+key.upper().replace('-','_')+': &[&str] = &[\n')
 lines.extend('    '+json.dumps(v)+',\n' for v in values);lines.append('];\n')
text=''.join(lines);(out/'properties.rs').write_text(text)
subprocess.run(['rustfmt','--edition','2024',str(out/'properties.rs')],check=True)
text=(out/'properties.rs').read_text()
report={'names':len(names),'block_size':block,'binary_bytes':len(payload),'binary_sha256':hashlib.sha256(payload).hexdigest(),'property_counts':{k:len(v) for k,v in registries.items()},'properties_rs_sha256':hashlib.sha256(text.encode()).hexdigest(),'full_name_round_trip_verified':True,'source_names_sha256':hashlib.sha256(raw).hexdigest(),'qualification':'Front-coded public semantic name facts from the pinned JVM. No implementation source is copied. Lookup is exact, not probabilistic.'}
(base/'generated-data.json').write_text(json.dumps(report,indent=2)+'\n')
with (base/'names.txt.gz').open('wb') as output:
 with gzip.GzipFile(filename='',fileobj=output,mode='wb',mtime=0) as archive:archive.write(raw)
print(report)
