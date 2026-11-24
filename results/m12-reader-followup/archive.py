"""Byte-preserving deterministic evidence archives; retain all local originals."""
from pathlib import Path
import gzip,hashlib,json,sys
OUT=Path(__file__).resolve().parent
if '--restore' in sys.argv:
 for entry in json.loads((OUT/'archives.json').read_text()):
  raw=gzip.decompress((OUT/entry['archive']).read_bytes())
  assert hashlib.sha256(raw).hexdigest()==entry['raw_sha256']
  path=OUT/entry['raw']
  if path.exists():assert path.read_bytes()==raw
  else:path.parent.mkdir(parents=True,exist_ok=True);path.write_bytes(raw)
else:
 entries=[]
 for path in sorted(OUT.rglob('*')):
  if not path.is_file() or path.suffix not in {'.json','.log','.edn','.wasm','.stderr','.stdout'} or path.name=='archives.json' or path.stat().st_size<40000:continue
  # Compile-time fixtures must remain available as raw files in a fresh checkout.
  if path.name in {'reader-inputs.json','reader-cases.json','uuid-runtime-cases.json','uuid-boundary-cases.json','uuid-parser-cases.edn','uuid-parser-inputs.edn'}:continue
  # Do not snapshot output while the owning command is live.
  receipts=[p/name for p in path.parents if p!=OUT for name in ('commands.json','command.json','suite-commands.json') if (p/name).exists()]
  if any(json.loads(receipt.read_text()).get('complete') is False for receipt in receipts):continue
  raw=path.read_bytes();archive=path.with_name(path.name+'.gz');archive.write_bytes(gzip.compress(raw,mtime=0));assert gzip.decompress(archive.read_bytes())==raw
  entries.append({'raw':str(path.relative_to(OUT)),'archive':str(archive.relative_to(OUT)),'raw_sha256':hashlib.sha256(raw).hexdigest(),'raw_bytes':len(raw),'archive_sha256':hashlib.sha256(archive.read_bytes()).hexdigest(),'archive_bytes':archive.stat().st_size})
 (OUT/'archives.json').write_text(json.dumps(entries,indent=2)+'\n')
 print('Archived',len(entries),'files; local raw originals retained')
