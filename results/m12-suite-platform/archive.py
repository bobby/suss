"""Create deterministic byte-preserving evidence archives; leave local raw files."""
from pathlib import Path
import gzip,hashlib,json
OUT=Path(__file__).resolve().parent
entries=[]
for p in sorted(OUT.rglob('*')):
 if not p.is_file() or p.suffix not in {'.json','.log','.wasm'} or p.stat().st_size<80000:continue
 if '__pycache__' in p.parts or p.name=='archives.json':continue
 raw=p.read_bytes();archive=p.with_name(p.name+'.gz');archive.write_bytes(gzip.compress(raw,mtime=0))
 assert gzip.decompress(archive.read_bytes())==raw
 entries.append({'raw':str(p.relative_to(OUT)),'archive':str(archive.relative_to(OUT)),'raw_sha256':hashlib.sha256(raw).hexdigest(),'raw_bytes':len(raw),'archive_sha256':hashlib.sha256(archive.read_bytes()).hexdigest()})
(OUT/'archives.json').write_text(json.dumps(entries,indent=2)+'\n')
print('Archived',len(entries),'raw receipts, preserving every byte')
