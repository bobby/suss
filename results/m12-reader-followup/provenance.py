"""Retain whole UUID upstream forms and exact native adaptations, with licenses."""
from pathlib import Path
import difflib,hashlib,json,sys,zipfile
ROOT=Path(__file__).resolve().parents[2];BASE=Path(__file__).resolve().parent
sys.path.insert(0,str(ROOT/'scripts'))
from core_import import PIN,Scanner,extract_forms,pinned_file
sha=lambda b:hashlib.sha256(b).hexdigest()
source=pinned_file(ROOT,'src/main/cljs/cljs/core.cljs').decode()
forms=extract_forms(source,'runtime','src/main/cljs/cljs/core.cljs')
original=next(f for f in forms.values() if f['name']=='UUID')
core=(ROOT/'docs/compatibility/bootstrap/exact-integers.sus').read_text()
a=core.index(';; BEGIN UUID');b=core.index(';; END UUID',a)+len(';; END UUID values and tagged-reader parser')
block=core[a:b]+'\n'
adapted=next(core[f.start:f.end] for f in Scanner(core).all() if f.kind=='list' and len(f.children)>1 and f.children[0].text=='deftype' and f.children[1].text=='UUID')
original_class=original['form']+'\n'
patch=''.join(difflib.unified_diff(original_class.splitlines(True),(adapted+'\n').splitlines(True),fromfile='cljs.core/UUID',tofile='suss.core/UUID'))
jar=Path(next(p for p in json.loads((BASE/'reader-oracle-command.json').read_text())['jars'] if p.endswith('clojure-1.12.6.jar')))
with zipfile.ZipFile(jar) as z:jvm=z.read('clojure/core.clj');uuid=z.read('clojure/uuid.clj')
text=jvm.decode();i=text.index('(defn uuid?');predicate=text[i:text.index('\n\n',i)]+'\n'
outputs={
 'docs/compatibility/uuid/upstream-UUID.cljs':original_class.encode(),
 'docs/compatibility/uuid/UUID.patch':patch.encode(),
 'docs/compatibility/uuid/upstream-uuid-predicate.clj':(uuid.decode().split('(ns ',1)[0]+predicate).encode(),
 'docs/compatibility/uuid/NOTICE':(source.split('\n\n')[0]+'\n').encode(),
}
record={
 'upstream_scaffold':{'revision':PIN,'source':'clojurescript/src/main/cljs/cljs/core.cljs','source_sha256':sha(source.encode()),'id':original['id'],'line':original['line'],'end_line':original['end-line'],'form_sha256':sha(original['form'].encode())},
 'primary':{'version':'org.clojure/clojure 1.12.6','jar_sha256':sha(jar.read_bytes()),'core_source_sha256':sha(jvm),'uuid_source_sha256':sha(uuid),'predicate_sha256':sha(predicate.encode()),'uuid_source_retained':'results/m12-reader-followup/clojure-uuid-primary.clj','evidence':'results/m12-reader-followup'},
 'license':{'upstream':'EPL-1.0','path':'runtime/core-import/epl-v10.html','notice':'docs/compatibility/uuid/NOTICE'},
 'adaptation':'Canonical nominal category and two immutable signed Long halves replace CLJS string storage and mutable cached hashing. Original UTF-16 parser, canonical formatter, Java-compatible bit hash and signed comparison follow executed JVM controls, not CLJS identity or caching. The type retains the Object, equality, hashing, comparison and printing protocol structure. No Java implementation or Java runtime dependency is shipped.',
 'block_sha256':sha(block.encode()),
 'adapted_class_sha256':sha(adapted.encode()),
 'files':{p:sha(raw) for p,raw in outputs.items()},
 'limits':'UUID integration only. Regex, instants, runtime eval and complete BigDecimal/suite acceptance retain their own evidence and limitations. Public parse-uuid/random-uuid APIs are not assessed by this adaptation.'}
outputs['docs/compatibility/bootstrap/clojure-uuid.json']=(json.dumps(record,indent=2)+'\n').encode()
for name,raw in outputs.items():
 path=ROOT/name
 if '--check' in sys.argv:assert path.read_bytes()==raw,name
 else:path.parent.mkdir(parents=True,exist_ok=True);path.write_bytes(raw)
print('UUID source provenance checked' if '--check' in sys.argv else 'UUID source provenance recorded')
