#!/usr/bin/env python3
"""Strict raw UTF16/effect evidence for String.charAt and unchanged whole get."""
import json
from pathlib import Path
import sys
from cljs_inventory import Scanner
ROOT=Path(__file__).resolve().parents[1]
PIN='c4295f303100bbf5afac449242d30bca1126f1a1'
CORPUS=ROOT/'tests/oracle/string-char-at-cases.json'
RAW=ROOT/'tests/oracle/out/string-char-at-observations.json'
EVIDENCE=ROOT/'docs/compatibility/string-char-at/evidence/primary-observations.json'

def recipes():
 rows=[]
 def add(identity,expression):
  source='[(try [false '+expression+'] (catch :default e [true e])) char-effects]'
  rows.append({'id':identity,'source':source})
 for label,s in [('ascii','"ab"'),('utf16','"A\\uD83D\\uDE00Z"'),('empty','""')]:
  for key,n in [('negative','-1'),('zero','0'),('fraction','1.5'),('one','1'),('two','2'),('three','3'),('past','4'),('huge','4294967297'),('nan','##NaN'),('infinity','##Inf'),('nil','nil'),('false','false'),('string','"1.2"')]:
   add('char-at-'+label+'-'+key,'(.charAt '+s+' '+n+')')
 for operation,expr in [('get-two','(get "ab" INDEX)'),('get-three','(get "ab" INDEX false)')]:
  for key,n in [('negative','-1'),('negative-half','-0.5'),('zero','0'),('one','1'),('past','2'),('nil','nil'),('false','false'),('string','"1"'),('huge','4294967297')]:add(operation+'-'+key,expr.replace('INDEX',n))
 add('get-suite-13','(get "ab" 0)')
 add('get-suite-28','(get "ab" 0 :not-found)')
 def tick(n):return '(set! char-effects (+ (* char-effects 10) '+str(n)+'))'
 obj='(js-obj "valueOf" (fn [] '+tick(1)+' 1.5) "toString" (fn [] '+tick(2)+' "0"))'
 for operation,expression in [('char-at','(.charAt "ab" INDEX)'),('get-two','(get "ab" INDEX)'),('get-three','(get "ab" INDEX false)')]:
  add(operation+'-object-index',expression.replace('INDEX',obj))
  add(operation+'-throw-index',expression.replace('INDEX','(js-obj "valueOf" (fn [] '+tick(1)+' (throw 705)) "toString" (fn [] '+tick(2)+' "0"))'))
 add('char-at-default-index','(.charAt "ab")')
 add('char-at-extra-argument','(.charAt "ab" 1 (do '+tick(7)+' 9))')
 add('char-at-ordered-receiver-index','(.charAt (do '+tick(1)+' "ab") (do '+tick(2)+' 1))')
 add('get-three-default-eager','(get "ab" 0 (do '+tick(3)+' false))')
 add('char-at-borrow-number','(.call (.-charAt "ab") 17 1)')
 add('char-at-borrow-false','(.call (.-charAt "ab") false 1)')
 add('char-at-borrow-object-ordered','(.call (.-charAt "ab") (js-obj "toString" (fn [] '+tick(2)+' "ab") "valueOf" (fn [] '+tick(1)+' 17)) '+obj+')')
 add('char-at-borrow-receiver-throw','(.call (.-charAt "ab") (js-obj "toString" (fn [] '+tick(2)+' (throw 706))) '+obj+')')
 add('char-at-method-identity','(identical? (.-charAt "a") (.-charAt "b"))')
 for row in rows:
  if len(Scanner(row['source']).all())!=1:raise ValueError('one complete expression required')
 return rows

def closed(v):
 if not isinstance(v,dict) or 'tag' not in v:raise ValueError('tagged value required')
 tag=v['tag']; fields={'nil':{'tag'},'bool':{'tag','value'},'f64':{'tag','bits'},'string':{'tag','units'},'vector':{'tag','items'}}
 if tag not in fields or set(v)!=fields[tag]:raise ValueError('unsupported/opaque value')
 if tag=='vector':
  for x in v['items']:closed(x)
 elif tag=='string':
  if not isinstance(v['units'],list) or any(type(x)!=int or not 0<=x<=65535 for x in v['units']):raise ValueError('UTF16 units')
 elif tag=='bool' and type(v['value'])!=bool:raise ValueError('boolean')
 elif tag=='f64':
  import re
  if not re.fullmatch('[0-9a-f]{16}',v['bits']):raise ValueError('binary64 bits')

def generate():
 rows=recipes();entries='\n'.join('#js {:id '+json.dumps(r['id'])+' :value (do (set! char-effects 0) (transport/encode '+r['source']+'))}' for r in rows)
 (ROOT/'tests/oracle/src/suss_oracle/string_char_at.cljs').write_text('(ns suss-oracle.string-char-at (:require [suss-oracle.main :as transport]))\n(def char-effects 0)\n(defn -main [] (println (.stringify js/JSON (array\n'+entries+'))))\n(set! *main-cli-fn* -main)\n')

def observations(path=RAW):
 values=json.loads(path.read_text())
 if not isinstance(values,list) or [v['id'] for v in values]!=[r['id'] for r in recipes()]:raise ValueError('complete ordered raw IDs')
 for v in values:
  if set(v)!={'id','value'}:raise ValueError('closed envelope')
  closed(v['value'])
 return values

def record():
 values=observations();rows=recipes()
 for r,v in zip(rows,values):r['expected']=v['value']
 CORPUS.write_text(json.dumps({'schema':1,'upstream':PIN,'cases':rows},indent=2)+'\n');print(len(rows),'fresh raw pin cases recorded')

def compare(path=RAW):
 values=observations(path);corpus=json.loads(CORPUS.read_text())
 if corpus['upstream']!=PIN or len(corpus['cases'])!=len(values):raise ValueError('corpus identity')
 for r,v,expected in zip(recipes(),values,corpus['cases']):
  if expected!={**r,'expected':v['value']}:raise ValueError('source/raw mismatch '+r['id'])
 print(len(values),'raw pin matches')

if __name__=='__main__':{'generate':generate,'record':record,'compare':compare}[sys.argv[1]]()
