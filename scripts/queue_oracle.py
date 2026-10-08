#!/usr/bin/env python3
"""Development-only strict queue oracle; generated output is not native evidence."""
import json
import math
from pathlib import Path
import sys
ROOT=Path(__file__).resolve().parents[1]
CORPUS=ROOT/'tests/oracle/queue-cases.json'
FOUNDATIONS=ROOT/'tests/oracle/out/queue-foundations-observations.json'
ITERATOR=ROOT/'tests/oracle/out/queue-iterator-observations.json'

def unique(pairs):
    result={}
    for key,value in pairs:
        if key in result: raise ValueError('duplicate JSON key: '+key)
        result[key]=value
    return result
def read(path):
    raw=path.read_bytes()
    if len(raw)>1<<20: raise ValueError('queue oracle input exceeds 1 MiB')
    return json.loads(raw, object_pairs_hook=unique, parse_constant=lambda value: (_ for _ in ()).throw(ValueError('nonfinite JSON number')))
def same(actual, expected):
    if type(expected) is bool: return type(actual) is bool and actual==expected
    if type(expected) in (int,float): return type(actual) in (int,float) and math.isfinite(actual) and actual==expected
    if type(actual) is not type(expected): return False
    if isinstance(expected,list): return len(actual)==len(expected) and all(same(a,e) for a,e in zip(actual,expected))
    if isinstance(expected,dict): return actual.keys()==expected.keys() and all(same(actual[k],v) for k,v in expected.items())
    return actual==expected
def load():
    corpus=read(CORPUS)
    if set(corpus)!= {'upstream','cases'} or corpus['upstream']!='c4295f303100bbf5afac449242d30bca1126f1a1': raise ValueError('queue corpus shape/pin')
    cases=corpus['cases']
    if not isinstance(cases,list) or len(cases)!=41: raise ValueError('queue case count')
    ids=set()
    for case in cases:
        if set(case)!= {'id','source','expected'} or not isinstance(case['id'],str) or not case['id'] or case['id'] in ids: raise ValueError('queue case identity')
        ids.add(case['id'])
        if not isinstance(case['source'],str) or not case['source'] or set(case['expected'])!= {'tag','value'} or case['expected']['tag']!='bool' or type(case['expected']['value']) is not bool: raise ValueError('queue expected Boolean shape')
    return cases

TRACE=[True,True,1,True,2,True,3,False,{'error':True,'message':'No such element'},{'error':True,'message':'Unsupported operation'},True]
def generate():
    cases=load()
    fixture=(ROOT/'tests/oracle/fixtures/queue-probe.sus').read_text()
    entries='\n'.join('#js {:id '+json.dumps(c['id'])+' :value '+c['source']+'}' for c in cases)
    target=ROOT/'tests/oracle/out/generated/suss_oracle/queue_foundations.cljs'
    target.parent.mkdir(parents=True,exist_ok=True)
    target.write_text('(ns suss-oracle.queue-foundations)\n'+fixture+'\n(defn -main [] (println (.stringify js/JSON (into-array ['+entries+']))))\n(set! *main-cli-fn* -main)\n')
def compare():
    cases=load()
    actual=read(FOUNDATIONS)
    expected=[{'id':c['id'],'value':c['expected']['value']} for c in cases]
    if not same(actual,expected): raise ValueError('queue complete ordered value observations differ')
    actual=read(ITERATOR)
    if not same(actual,TRACE): raise ValueError('queue complete ordered iterator observations differ')
    print('PASS 41 queue value cases and 11 complete ordered iterator observations')
if __name__=='__main__':
    {'generate':generate,'compare':compare}[sys.argv[1]]()
