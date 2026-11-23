"""Preserve original JSON layout after the separately audited semantic write."""
from pathlib import Path
import difflib,gzip,hashlib,json
ROOT=Path(__file__).resolve().parents[2];OUT=Path(__file__).resolve().parent/'suite-after-manifest-refresh'
path=ROOT/'tests/oracle/clojure-test-suite-known-failures.json'
old_raw=gzip.decompress((OUT/'reviewed-before-known-failures.json.gz').read_bytes());old=json.loads(old_raw)
assert old_raw==(json.dumps(old,indent=1)+'\n').encode()
expected=json.loads(old_raw);del expected['namespaces']['clojure.core-test.case']
current=path.read_bytes();assert json.loads(current)==expected
minimal=(json.dumps(expected,indent=1)+'\n').encode()
removed=list(difflib.unified_diff(old_raw.decode().splitlines(),minimal.decode().splitlines(),n=0))
assert sum(line.startswith('-') and not line.startswith('---') for line in removed)==4
assert not any(line.startswith('+') and not line.startswith('+++') for line in removed)
path.write_bytes(minimal)
(OUT/'baseline-layout.json').write_text(json.dumps({'previous_serialization_sha256':hashlib.sha256(current).hexdigest(),
 'final_baseline_sha256':hashlib.sha256(minimal).hexdigest(),'semantic_change_from_previous_write':False,
 'delta_from_original':'Remove exactly four lines for the resolved case namespace; all other bytes preserved'},indent=2)+'\n')
print('Preserved original layout; exactly four removed lines')
