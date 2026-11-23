from pathlib import Path
import collections,gzip,hashlib,json,re
ROOT=Path(__file__).resolve().parents[2];OUT=Path(__file__).resolve().parent
path=ROOT/'tests/oracle/clojure-test-suite-observations.json'
reference=json.loads(path.read_text())
baseline=json.loads(gzip.decompress((OUT/'reviewed-before-known-failures.json.gz').read_bytes()))['assertions']
def tags(x):
 if isinstance(x,dict):return {x.get('tag')}|set().union(*(tags(v) for v in x.values()))
 if isinstance(x,list):return set().union(*(tags(v) for v in x))
 return set()
required=re.compile(r'\((?:clojure.core/)?(?:numerator|denominator|rationalize|bigint|biginteger|ratio\?|rational\?|integer\?)\s')
rows=[];counts=collections.Counter();required_non_decimal=[]
for r in reference['assertions']:
 reason=baseline.get(r['id']);source=r.get('form','');ts=tags(r)
 if not reason:continue
 decimal=bool({'decimal','bigdecimal'}&ts or re.search(r'\dM\b',source))
 if required.search(source) and not decimal:required_non_decimal.append({'id':r['id'],'form':source,'reason':reason})
 if not ({'bigint','ratio'}&ts or re.search(r'\dN\b|\d/\d',source) or required.search(source)):continue
 category='count-guarded' if reason.startswith('count:') else 'not-executed' if reason.startswith(('namespace:','test:','missing')) else 'known-failure'
 counts[(category,decimal)]+=1
 rows.append({'id':r['id'],'form':source,'reason':reason,'category':category,'decimal':decimal})
report={'reference_sha256':hashlib.sha256(path.read_bytes()).hexdigest(),'counts':[{'category':k[0],'decimal':k[1],'count':v} for k,v in counts.items()],'rows':rows,'required_function_non_decimal_failures':required_non_decimal}
(OUT/'scoped-failure-audit.json').write_text(json.dumps(report,indent=2)+'\n')
print('scoped counts',report['counts'],'remaining required-function non-Decimal failures',len(required_non_decimal))
