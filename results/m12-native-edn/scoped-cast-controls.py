from pathlib import Path
import gzip,hashlib,json,re,subprocess
ROOT=Path(__file__).resolve().parents[2];OUT=Path(__file__).resolve().parent
reference_path=ROOT/'tests/oracle/clojure-test-suite-observations.json'
reference=json.loads(reference_path.read_text())
baseline=json.loads(gzip.decompress((OUT/'reviewed-before-known-failures.json.gz').read_bytes()))['assertions']
rows=[]
for row in reference['assertions']:
 reason=baseline.get(row['id'],'')
 if reason.startswith('count:') and re.search(r'\dN\b|\d/\d',row.get('form','')) and not re.search(r'\dM\b',row.get('form','')):
  source=row['form'];adapted=source
  match=re.fullmatch(r'\(p/thrown\? (.*)\)',source)
  if match: adapted='(try '+match[1]+' false (catch :default e true))'
  applicable=source!='(instance? java.lang.Float (float 0N))'
  rows.append({'id':row['id'],'original':source,'source':adapted,'full_suite_reason':reason,'applicable':applicable,'limitation':None if applicable else 'ADR-0003 Types: no narrow boxed values; this :clj Float class assertion has no Suss equivalent.'})
assert len(rows)==31 and sum(r['applicable'] for r in rows)==30
jars=[Path('/home/bobby/.m2/repository')/p for p in ['org/clojure/clojure/1.12.6/clojure-1.12.6.jar','org/clojure/spec.alpha/0.5.238/spec.alpha-0.5.238.jar','org/clojure/core.specs.alpha/0.4.74/core.specs.alpha-0.4.74.jar']]
# Use the vendored assertion extension in the primary process, so p/thrown?
# records the original suite's report rather than interpreting a return value.
program="(require '[clojure.test :as t] '[clojure.core-test.portability :as p])\n"+'\n'.join('(let [reports (atom [])] (with-redefs [t/report (fn [m] (swap! reports conj (:type m)))] (t/is '+r['original']+')) (prn (= [:pass] @reports)))' for r in rows)
command=['java','-cp',':'.join([*map(str,jars),str(ROOT/'vendor/clojure-test-suite/upstream/test')]),'clojure.main','-e',program]
p=subprocess.run(command,text=True,capture_output=True)
(OUT/'scoped-cast-oracle.json').write_text(json.dumps({'command':command,'reference_sha256':hashlib.sha256(reference_path.read_bytes()).hexdigest(),'jars':{str(p):hashlib.sha256(p.read_bytes()).hexdigest() for p in jars},'exit_code':p.returncode,'stdout':p.stdout,'stderr':p.stderr},indent=2)+'\n')
assert p.returncode==0,p.stderr
values=p.stdout.splitlines();assert len(values)==len(rows),(len(values),p.stdout)
for row,value in zip(rows,values):
 assert value=='true',(row,value)
 row['expected']=value
(OUT/'scoped-cast-source-audit.json').write_text(json.dumps(rows,indent=2)+'\n')
(OUT/'scoped-cast-cases.json').write_text(json.dumps([r for r in rows if r['applicable']],indent=2)+'\n')
print('31 fresh primary assertion reports; 30 applicable isolated controls; one explicit ADR platform class case')
