"""Supplemental selected-source reference; preserve all actual primary failures.

The global primary normalizer correctly requires a fully passing JVM/clj suite.
This separate complete default-num experiment has three actual JVM narrow-box
failures. Retain those verdicts/values, verify all four independent recordings,
and add only the same identity/canonical metadata used by the global comparator.
Never rewrite or replace the original global reference or its normalization.
"""
from pathlib import Path
import hashlib,json,sys
here=Path(__file__).resolve().parent;root=here.parents[2];sys.path.insert(0,str(root/'scripts'))
import clojure_test_suite as suite
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
folders=[here]+[here/f'run{n}' for n in [2,3,4]]
runs=[];inputs={}
for folder in folders:
 receipt=json.loads((folder/'command.json').read_text());run=json.loads((folder/'observations.json').read_text())
 assert receipt['exit_code']==1 and receipt['inputs_unchanged']
 assert run['summary']=={'test':1,'pass':23,'fail':3,'error':0}
 assert run['feature-selection']==['suss','default'] and not run['reporter-errors']
 assert len(run['assertions'])==26 and [r['ordinal'] for r in run['assertions']]==list(range(1,27))
 assert [r['ordinal'] for r in run['assertions'] if r['verdict']=='fail']==[7,8,9]
 for f in folder.iterdir():
  if f.is_file() and f.name not in ['reference.json','reference-build.json']:inputs[str(f.relative_to(root))]=sha(f)
 runs.append(dict(run,assertions=[suite.canonical_assertion(row) for row in run['assertions']]))
assert all(run==runs[0] for run in runs)
reference=dict(runs[0]);rows=[]
for row in reference['assertions']:
 entry={key:row[key] for key in ['test','ordinal','kind','verdict','line','column','contexts']}
 entry.update(id=f"{row['test']}#{row['ordinal']}",deterministic=True,form=suite.canonical_form(row['form']))
 entry.update({key:row[key] for key in suite.OBSERVATION_KEYS[row['kind']]})
 rows.append(entry)
reference['assertions']=rows;output=here/'reference.json';output.write_text(json.dumps(reference,indent=2)+'\n')
(here/'reference-build.json').write_text(json.dumps(dict(inputs=inputs,standard_comparator_sha256=sha(root/'scripts/clojure_test_suite.py'),output_sha256=sha(output),runs=4,primary_exit_codes=[1]*4,preserved_primary_failures=[7,8,9],qualification='Supplemental complete original Suss/default-selected num body. All actual JVM verdicts/values retained. No global primary, comparator, counts, skip rules, opaque rules or baseline is modified.'),indent=2)+'\n')
print('Supplemental reference:26 rows,23 actual passes,3 actual failures retained,4 identical pinned recordings')
