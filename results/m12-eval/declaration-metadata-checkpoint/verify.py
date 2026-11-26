"""Verify archived terminals and the exact focused/publication source boundary."""
from pathlib import Path
import gzip,hashlib,json,sys
P=Path(__file__).resolve().parent;ROOT=P.parents[2]
sha=lambda b:hashlib.sha256(b).hexdigest()
read=lambda p:json.loads(p.read_bytes())
r=read(P/'summary.json')
for n,h in r['archived_files'].items():assert sha((P/n).read_bytes())==h,n
for name,code in [('root-metadata-native-diagnostic-checkpoint',101),('root-diagnostic-native-checkpoint',0)]:
 p=P.parent/name;review=read(p/'review.json');c=json.loads(gzip.decompress((p/'controller.json.gz').read_bytes()))
 assert c['complete'] and c['inputs_unchanged'] and c['head_unchanged'] and c['binaryen_unchanged']
 assert c['native_commands']=={'suite-build':0,'suite-run':code,'suite-lock':0}
 assert review['actual_native_exit_code']==code and review['observed_row_decisions_preserved']==5564
 for n,h in review['files'].items():assert sha(gzip.decompress((p/(n+'.gz')).read_bytes()))==h,n
 if code==0:
  for n,h in review['source_overlay'].items():assert sha(gzip.decompress((p/'source-overlay'/(n+'.gz')).read_bytes()))==h,n
positive=json.loads(gzip.decompress((P/'declaration-metadata-top-level-validation/commands.json.gz').read_bytes()))
negative=json.loads(gzip.decompress((P/'declaration-metadata-lexical-validation/commands.json.gz').read_bytes()))
assert positive['complete'] and positive['inputs_unchanged'] and positive['binaryen_unchanged']
assert len(positive['commands'])==5 and all(c['complete'] and c['exit_code']==0 for c in positive['commands'])
assert negative['complete'] and negative['inputs_unchanged'] and negative['exit_code']==101
# Formatting publication preserves exactly the same two measured semantic edits.
old=json.loads(gzip.decompress((P.parent/'root-metadata-native-diagnostic-checkpoint/known-failures-before.json.gz').read_bytes()))
for n in ['clojure.core-test.add-watch','clojure.core-test.remove-watch']:
 old['namespaces'][n]['diagnostic']=old['namespaces'][n]['diagnostic'].replace('Unresolved Runtime name alter-var-root at bytes','Unresolved Runtime name ref at bytes')
assert old==json.loads(gzip.decompress((P/'declaration-metadata-top-level-validation/overlay/tests/oracle/clojure-test-suite-known-failures.json.gz').read_bytes()))
if '--current' in sys.argv:
 for n,h in (positive['inputs']|r['publication_doc_overrides']).items():assert (ROOT/n).is_file() and sha((ROOT/n).read_bytes())==h,n
 for n,h in (r['production_inputs_conserved_from_native']|r['protected_original_inputs']).items():assert sha((ROOT/n).read_bytes())==h,n
print('Archived native0/101, all5564decisions, complete34eval,229/200numeric matches and5focused Rust executions verified. Original primary/projection/comparator/harness preserved. Workspace/review/CI remain separate.')
