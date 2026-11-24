"""Require actual native0 and the unchanged strict complete-capture contract."""
from pathlib import Path
import gzip,hashlib,importlib.util,json,subprocess,sys
ROOT=Path(__file__).resolve().parents[2]
sha=lambda raw:hashlib.sha256(raw).hexdigest()
HARNESS_SHA={'scripts/clojure_test_suite.py': '197166b70c16a1bc5e7a82d5061b069b5ba7a45453056a014704b1c345de617b', 'crates/suss-compile/tests/clojure_test_suite.rs': '2795a90dfacf9cec41d2d76addd4cdfc83235ca31b6cc37502275ed37207c559'}
def main():
 out=Path(sys.argv[1]).resolve();receipt=json.loads((out/'suite-commands.json').read_text())
 assert receipt['complete'] and receipt['inputs_unchanged']
 assert receipt['head']==subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip()
 assert all((ROOT/p).is_file() and sha((ROOT/p).read_bytes())==digest for p,digest in receipt['inputs'].items())
 commands={entry['label']:entry for entry in receipt['commands']}
 assert set(commands)=={'suite-build','suite-run','suite-lock'} and all(entry['exit_code']==0 for entry in commands.values()),'require actual native0, not only producer0'
 for path,digest in HARNESS_SHA.items():assert sha((ROOT/path).read_bytes())==digest,path
 spec=importlib.util.spec_from_file_location('strict_complete_capture',ROOT/'results/m12-eval/review-complete-capture.py')
 reviewer=importlib.util.module_from_spec(spec);spec.loader.exec_module(reviewer)
 original=ROOT/'results/m12-eval/eval-broader-after-symbol-and-var-contract/complete-capture'
 primary=(ROOT/'tests/oracle/clojure-test-suite-observations.json').read_bytes()
 baseline=gzip.decompress((original/'parent-random-baseline.json.gz').read_bytes())
 previous=gzip.decompress((ROOT/'results/m12-reader-followup/full-suite-after-random-and-assert/suite-capture/suite-observations.json.gz').read_bytes())
 raw=(out/'suite-capture/suite-observations.json').read_bytes()
 report=reviewer.review(primary,baseline,previous,raw)
 current=(ROOT/'tests/oracle/clojure-test-suite-known-failures.json').read_bytes()
 assert sha(current)=='dea5db93e9548008d91e5c79ee11978920575975ec601f1852139ff063329d7f'
 assert report.pop('proposed_baseline')==json.loads(current),'no new accepted failure change'
 sys.path.insert(0,str(ROOT/'scripts'));import clojure_test_suite as suite
 old=json.loads(gzip.decompress((original/'suite-capture/suite-observations.json.gz').read_bytes()))
 observed=json.loads(raw);reference=json.loads(primary)
 assert [(row['test'],row['ordinal']) for row in observed['assertions']]==[(row['test'],row['ordinal']) for row in old['assertions']],'preserve all5564 ordered observed row identities'
 assert suite.compare(reference,old)==suite.compare(reference,observed),'preserve all count/skip/guest/failure classifications'
 report.update(head=receipt['head'],capture_sha256=sha(raw),baseline_sha256=sha(current),original_native_exit_code=0,baseline_written=False,qualification='Fresh actual native0 on the current frozen source. The unchanged strict reviewer preserves all previous comparable successes, exact complete eval/symbol/Var bodies, all229/200 independently matching numeric tagged rows and the existing measured baseline. No new failures, skips or opaque judgments are accepted. Numeric source-form scope and workspace/review/CI remain separate gates.')
 target=out/'current-complete-capture-review.json';assert not target.exists();target.write_text(json.dumps(report,indent=2,sort_keys=True)+'\n');print(json.dumps(report,indent=2))
if __name__=='__main__':main()
