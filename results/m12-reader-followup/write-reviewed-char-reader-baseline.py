"""Record only the two executed namespaces and their explicit reader failures."""
from pathlib import Path
import hashlib,json,subprocess,sys
ROOT=Path(__file__).resolve().parents[2];OUT=ROOT/'results/m12-reader-followup'/sys.argv[1]
sys.path.insert(0,str(ROOT/'scripts'));import clojure_test_suite as suite
path=ROOT/'tests/oracle/clojure-test-suite-known-failures.json'
old_bytes=subprocess.check_output(['git','show','8bc237b72669639876483077ebe53e01962a0dea:'+str(path.relative_to(ROOT))],cwd=ROOT)
assert path.read_bytes()==old_bytes,'baseline has changed since the published source snapshot'
old=json.loads(old_bytes)
receipt=json.loads((OUT/'suite-commands.json').read_text())
assert receipt['head']=='bc2694b92ee100d97101736d098a0e9ff03099e5'
assert receipt['complete'] and receipt['inputs_unchanged']
edn=json.loads((OUT/'edn-body-audit.json').read_text())
char=json.loads((OUT/'char-body-audit.json').read_text())
assert char['char_assertions']==char['host_matches']==9 and char['guest_judgments']==char['missing']==0
assert edn['edn_assertions']==297 and edn['ordered_ids_match']
assert edn['outcomes']=={'host-match':272,'guest-judged':6,'kind:error':17,'observation:value':1,'verdict:fail':1}
observed_raw=(OUT/'suite-capture/suite-observations.json').read_bytes()
assert edn['capture_sha256']==char['capture_sha256']==hashlib.sha256(observed_raw).hexdigest()
reference=json.loads((ROOT/'tests/oracle/clojure-test-suite-observations.json').read_bytes())
observed=json.loads(observed_raw);counts,measured=suite.compare(reference,observed)
assert counts==edn['counts']==char['full_suite_counts']
proposed=json.loads((OUT/'proposed-known-failures.json').read_text());assert proposed==measured
expected=json.loads(old_bytes)
for ns in ['clojure.core-test.char','clojure.edn-test.read-string']:
 assert ns in expected['namespaces'];del expected['namespaces'][ns]
for row in edn['explicit_remaining_failures']:
 assert row['id'] not in expected['assertions'];expected['assertions'][row['id']]=row['reason']
assert len(edn['explicit_remaining_failures'])==19
assert expected==measured,'unexpected unrelated baseline change'
assert old_bytes==(json.dumps(old,indent=1)+'\n').encode(),'preserve baseline formatting'
path.write_text(json.dumps(expected,indent=1)+'\n')
assert suite.compare(reference,observed)[1]==json.loads(path.read_text())
report={'old_sha256':hashlib.sha256(old_bytes).hexdigest(),'new_sha256':hashlib.sha256(path.read_bytes()).hexdigest(),'capture_sha256':edn['capture_sha256'],'counts':counts,'char_host_matches':9,'edn_assertions':297,'reader_failures_retained':19,'host_comparison_exit_code':0,'original_native_capture_exit_code':101,'change':'Remove only char and EDN namespace-load failures after actual complete execution; add exactly the 19 independently observed reader failures. Preserve all unrelated failures and every primary/comparator/count/skip rule.','qualification':'This baseline write and host comparison are not a native rerun or a full workspace pass.'}
out=OUT/'reviewed-baseline-write.json';assert not out.exists();out.write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps(report,indent=2))
