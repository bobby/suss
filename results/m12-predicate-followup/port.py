"""Add reviewed ports through the existing reproducible core importer."""
from pathlib import Path
import json,sys
ROOT=Path(__file__).resolve().parents[2]
sys.path.insert(0,str(ROOT/'scripts'))
import core_import
source=(ROOT/'clojurescript/src/main/cljs/cljs/core.cljs').read_text()
forms=core_import.extract_forms(source,'runtime','clojurescript/src/main/cljs/cljs/core.cljs')
recipe_path=ROOT/'docs/compatibility/core-import.json'
recipe=json.loads(recipe_path.read_text())
overlay_path=ROOT/'docs/compatibility/reviews.edn'
overlay=overlay_path.read_text()
entries=[]
for name in ['char?','coll?','sorted?','pos-int?','neg-int?','nat-int?']:
 form=next(f for f in forms.values() if f['name']==name)
 digest=core_import.digest(form['form'].encode())
 stem=name.replace('?','-predicate')
 patch_path=f'docs/compatibility/patches/core-{stem}.json'
 if name in ['char?','coll?','sorted?']:
  # Preserve the entire pinned body and docstring; only declaration expansion.
  start=form['form'].index('  [')
  before=form['form'][:start].replace('(defn '+name,'(def '+name,1)
  replacement=before+'  (fn '+form['form'][start+2:]+')'
  deps={'char?':['runtime:string?','runtime:=='],'coll?':['macro:nil?','runtime:ICollection:630','macro:satisfies?'],'sorted?':['runtime:ISorted','macro:satisfies?']}[name]
  rationale='Preserve the complete pinned docstring and body; expand defn into def/fn for the existing compiled core pipeline.'
  if name=='char?':
   rationale+=' The accepted design represents character literals as one-unit UTF-16 strings; preserve its one-unit string test, including lone surrogates. JVM Character identity and the distinction between a one-unit String and Character are not fabricated; the two vendored String judgments remain explicit platform differences.'
 else:
  predicate={'pos-int?':'(pos? x)','neg-int?':'(neg? x)','nat-int?':'(not (neg? x))'}[name]
  doc={'pos-int?':'Return true if x satisfies int? and is positive.','neg-int?':'Return true if x satisfies int? and is negative.','nat-int?':'Return true if x satisfies int? and is a natural integer value.'}[name]
  meta='^boolean ' if name=='neg-int?' else ''
  replacement=f'(def {meta}{name}\n  {json.dumps(doc)}\n  (fn [x] (if (suss.bootstrap/long? x) {predicate} false)))'
  deps=['suss.bootstrap/long?','runtime:pos?:3077' if name=='pos-int?' else 'runtime:neg?:3086']
  if name=='nat-int?':deps.append('runtime:not:263')
  rationale='Original Clojure numeric-category adaptation under ADR-0003. Preserve the public docstring, one-argument function and sign predicate. Fixed-width integers are canonical Long values; BigInt/BigInteger, Ratio, Decimal and Double return false even when integral. Replace the JavaScript integer?/goog.math wrapper representation dispatch with the immutable private Long representation test, retaining short-circuit sign evaluation. Fresh JVM Clojure1.12.6 controls are the behavior oracle; no Java dependency is shipped.'
 (ROOT/patch_path).write_text(json.dumps({'schema':1,'source-sha256':digest,'replacement':replacement,'rationale':rationale},indent=2)+'\n')
 assert all(f['id']!=form['id'] for f in recipe['forms'])
 recipe['forms'].append({'id':form['id'],'source-sha256':digest,'patch':patch_path})
 entry=f'''  {json.dumps(form['id'])}
  {{:source-sha256 {json.dumps(digest)}
   :visibility :public :arities {{:fixed [1] :variadic-min nil}}
   :dependencies {json.dumps(deps).replace(', ', ' ')}
   :classification :adapted
   :rationale {json.dumps(rationale)}
   :status :in-progress :adaptation-path {json.dumps(patch_path)}
   :alternative nil
   :tests ["crates/suss-cli/tests/clojure_core_predicates.rs"]}}
'''
 assert json.dumps(form['id']) not in overlay
 entries.append(entry)
recipe_path.write_text(json.dumps(recipe,indent=2)+'\n')
needle=' :reviews {\n';assert overlay.count(needle)==1
overlay_path.write_text(overlay.replace(needle,needle+''.join(entries),1))
