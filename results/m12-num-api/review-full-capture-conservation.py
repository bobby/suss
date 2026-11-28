"""Audit actual native101 without accepting differences or writing a baseline."""
from pathlib import Path
import collections,gzip,hashlib,json,sys
ROOT=Path(__file__).resolve().parents[2]
sys.path.insert(0,str(ROOT/'scripts'))
import clojure_test_suite as suite
sha=lambda raw:hashlib.sha256(raw).hexdigest()
def archived(path):
 return path.read_bytes() if path.exists() else gzip.decompress(path.with_name(path.name+'.gz').read_bytes())
def main():
 out=Path(sys.argv[1]);primary=(ROOT/'tests/oracle/clojure-test-suite-observations.json').read_bytes();baseline=(gzip.decompress((out/'baseline-before-expansion.json.gz').read_bytes()) if (out/'baseline-before-expansion.json.gz').exists() else (ROOT/'tests/oracle/clojure-test-suite-known-failures.json').read_bytes())
 assert sha(primary)=='449264ab6bca1dcf761d0204e4a64de4de91562e576615c4e84a63246839f4c7'
 assert sha(baseline)=='dea5db93e9548008d91e5c79ee11978920575975ec601f1852139ff063329d7f'
 receipt=json.loads(archived(out/'capture/suite-commands.json'));assert receipt['complete'] and receipt['inputs_unchanged'];assert [r['exit_code'] for r in receipt['commands']]==[0,101,0]
 oldraw=gzip.decompress((out/'previous-eval-capture.json.gz').read_bytes());assert sha(oldraw)=='4e8b197fcb6c6dfc4d90a0a9f1e6fd0586d84bcc5b97d8b9e279dd0c94b5cc69'
 newraw=archived(out/'capture/suite-capture/suite-observations.json');reference,old,new=map(json.loads,[primary,oldraw,newraw]);before,oldfail=suite.compare(reference,old);after,newfail=suite.compare(reference,new);assert oldfail==json.loads(baseline)
 refs={r['id']:r for r in reference['assertions']};observed={r['test']+'#'+str(r['ordinal']):r for r in new['assertions']};assert len(observed)==len(new['assertions'])
 oldids=[r['test']+'#'+str(r['ordinal']) for r in old['assertions']];oldset=set(oldids);assert [r['test']+'#'+str(r['ordinal']) for r in new['assertions'] if r['test']+'#'+str(r['ordinal']) in oldset]==oldids
 oldcounts=collections.Counter(r['test'] for r in old['assertions']);newcounts=collections.Counter(r['test'] for r in new['assertions']);assert all(newcounts[k]==n for k,n in oldcounts.items())
 namespaces={r['namespace']:r for r in new['namespaces']};assert all(namespaces[r['namespace']]==r for r in old['namespaces'] if r['status']=='loaded')
 preserved=collections.Counter()
 for row in old['assertions']:
  key=row['test']+'#'+str(row['ordinal']);decision=suite.decide(refs[key],row)
  if decision[0] is None:
   assert suite.decide(refs[key],observed[key])==decision,key
   preserved['guest' if decision[1] else 'host-compatible-row']+=1
 added=[k for k in observed if k not in oldset]
 assert collections.Counter(observed[k]['test'] for k in added)=={'clojure.core-test.eq/test-eq':71,'clojure.core-test.not-eq/test-not-eq':71,'clojure.core-test.num/test-num':26,'clojure.core-test.remove/test-remove':18}
 for test in ['clojure.core-test.eq/test-eq','clojure.core-test.not-eq/test-not-eq']:
  assert all(suite.decide(refs[k],observed[k])==(None,False) for k in added if observed[k]['test']==test)
 def tagged(node,tags):
  if isinstance(node,dict):return node.get('tag') in tags or any(tagged(v,tags) for v in node.values())
  return isinstance(node,list) and any(tagged(v,tags) for v in node)
 numeric={}
 for name,tags,total in [('bigint-ratio',{'bigint','biginteger','ratio'},229),('decimal',{'decimal'},200)]:
  selected=[r for r in reference['assertions'] if tagged(r,tags)];assert len(selected)==total
  assert all(suite.decide(r,observed[r['id']])==(None,False) for r in selected)
  numeric[name]=total
 assert {k for k,v in oldfail.items() if v!=newfail[k]}=={'namespaces','assertions'}
 assert newfail['assertions']=={**oldfail['assertions'],**{'clojure.core-test.num/test-num#'+str(n):'count:26/18' if n<=18 else 'unexpected-assertion' for n in range(1,27)},'clojure.core-test.remove/test-remove#18':'verdict:fail'}
 print(json.dumps(dict(before=before,after=after,preserved_row_decisions=preserved,added_rows=len(added),numeric_tagged_host_matches=numeric,primary_sha256=sha(primary),baseline_sha256=sha(baseline),capture_sha256=sha(newraw),actual_native_exit=101,baseline_written=False,qualification='Read-only conservation audit only. All old test counts/ordered rows/loaded namespaces and successful row judgments are preserved. Newly visible count/profile and character failures remain actual failures; no baseline acceptance, comparison/count/skip/opaque-rule change or native0 claim.'),indent=2))
if __name__=='__main__':main()
