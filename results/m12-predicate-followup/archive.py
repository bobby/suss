"""Deterministically archive large receipts; never delete the original evidence."""
from pathlib import Path
import gzip,hashlib,json,sys
OUT=Path(__file__).resolve().parent
index=OUT/'archives.json'
if '--restore' in sys.argv:
 for row in json.loads(index.read_text()):
  path=OUT/row['path'];raw=gzip.decompress((OUT/row['archive']).read_bytes());assert hashlib.sha256(raw).hexdigest()==row['sha256']
  if path.exists():assert path.read_bytes()==raw
  else:path.parent.mkdir(parents=True,exist_ok=True);path.write_bytes(raw)
else:
 rows=[]
 for path in sorted(OUT.rglob('*')):
  if path.suffix not in ['.json','.log'] or path.name in ['archives.json']:continue
  raw=path.read_bytes()
  # Fixtures and concise audits remain directly readable. Raw source receipts,
  # full captures and verbose Cargo artifact streams get byte-exact gzip copies.
  if len(raw)<100_000:continue
  compressed=gzip.compress(raw,mtime=0);archive=Path(str(path)+'.gz')
  if archive.exists():assert archive.read_bytes()==compressed
  else:archive.write_bytes(compressed)
  rows.append({'path':str(path.relative_to(OUT)),'archive':str(archive.relative_to(OUT)),'bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest(),'archive_sha256':hashlib.sha256(compressed).hexdigest()})
 index.write_text(json.dumps(rows,indent=2)+'\n');print({'archives':len(rows),'uncompressed_bytes':sum(x['bytes'] for x in rows),'archive_bytes':sum((OUT/x['archive']).stat().st_size for x in rows)})
