"""Record only the independently audited removal of the case namespace failure."""
from pathlib import Path
import gzip,hashlib,json,subprocess,sys
ROOT=Path(__file__).resolve().parents[2];OUT=Path(__file__).resolve().parent/'suite-after-manifest-refresh'
sys.path.insert(0,str(ROOT/'scripts'));import clojure_test_suite as suite
path=ROOT/'tests/oracle/clojure-test-suite-known-failures.json'
old_bytes=gzip.decompress((OUT/'reviewed-before-known-failures.json.gz').read_bytes());old=json.loads(old_bytes)
expected=json.loads(old_bytes);del expected['namespaces']['clojure.core-test.case']
proposed=json.loads((OUT/'proposed-known-failures.json').read_text());assert proposed==expected
assert path.read_bytes()==old_bytes, 'baseline changed since the captured audit'
primary_path=ROOT/'tests/oracle/clojure-test-suite-observations.json';primary=primary_path.read_bytes()
assert hashlib.sha256(primary).hexdigest()=='449264ab6bca1dcf761d0204e4a64de4de91562e576615c4e84a63246839f4c7'
observed=json.loads((OUT/'suite-capture/suite-observations.json').read_bytes())
counts,measured=suite.compare(json.loads(primary),observed);assert measured==expected
path.write_text(json.dumps(expected,indent=2,sort_keys=True)+'\n')
assert suite.compare(json.loads(primary),observed)[1]==json.loads(path.read_text())
receipt={'change':'Remove only clojure.core-test.case namespace failure; preserve all assertion/test failures and skip guards',
 'old_sha256':hashlib.sha256(old_bytes).hexdigest(),'new_sha256':hashlib.sha256(path.read_bytes()).hexdigest(),
 'primary_sha256':hashlib.sha256(primary).hexdigest(),'counts':counts,'host_comparison_exit_code':0,
 'original_capture_rust_exit_code':101,'qualification':'The original Rust drift exit is retained; this is a separate host audit and explicit baseline update, not a rerun of the original Rust test.'}
(OUT/'reviewed-baseline-write.json').write_text(json.dumps(receipt,indent=2)+'\n');print(json.dumps(receipt,indent=2))
