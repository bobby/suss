#!/usr/bin/env python3
"""Independent host-shaped observations; native compilation/execution remains pending."""
import json
import re
import sys
from pathlib import Path
from cljs_inventory import Scanner
from tree_node_oracle import unique
ROOT = Path(__file__).resolve().parents[1]
PIN = 'c4295f303100bbf5afac449242d30bca1126f1a1'

def read(path):
    return json.loads(path.read_text(), object_pairs_hook=unique)

def observation(v, depth=0):
    if depth > 64 or not isinstance(v, dict):
        raise ValueError('observation depth/type')
    tag = v.get('tag')
    keys = {'nil': {'tag'}, 'bool': {'tag', 'value'}, 'number': {'tag', 'bits'},
            'node': {'tag', 'class', 'key', 'val', 'left', 'right', 'hash'},
            'reduced': {'tag', 'value'}, 'vector': {'tag', 'items'}, 'string': {'tag', 'units'}, 'error': {'tag', 'class', 'message'}}
    if tag not in keys or set(v) != keys[tag]:
        raise ValueError('closed observation schema')
    if tag == 'bool' and type(v['value']) is not bool:
        raise ValueError('Boolean type')
    if tag == 'number' and (type(v['bits']) is not str or not re.fullmatch('[0-9a-f]{16}', v['bits'])):
        raise ValueError('lossless number bits')
    if tag == 'node':
        if v['class'] not in ('BlackNode', 'RedNode'):
            raise ValueError('node class')
        for key in ('key', 'val', 'left', 'right', 'hash'):
            observation(v[key], depth + 1)
        if any(v[key]['tag'] not in ('nil', 'node') for key in ('left', 'right')):
            raise ValueError('tree child type')
    if tag == 'vector':
        if not isinstance(v['items'], list) or len(v['items']) > 4096: raise ValueError('vector bound')
        for item in v['items']: observation(item, depth+1)
    if tag == 'string':
        if not isinstance(v['units'], list) or len(v['units']) > 4096 or any(type(u) is not int or not 0 <= u <= 65535 for u in v['units']): raise ValueError('UTF16 string')
    if tag == 'reduced':
        observation(v['value'], depth + 1)
    if tag == 'error' and (v['class'] != 'Error' or not isinstance(v['message'], list)
            or len(v['message']) > 4096 or any(type(u) is not int or not 0 <= u <= 65535 for u in v['message'])):
        raise ValueError('Error class/UTF16')

def cases():
    c = read(ROOT / 'tests/oracle/sorted-tree-cases.json')
    if set(c) != {'schema', 'upstream', 'cases'} or type(c['schema']) is not int or c['schema'] != 1 or c['upstream'] != PIN:
        raise ValueError('pin/schema')
    rows = c['cases']
    if not isinstance(rows, list) or len(rows) != 142 or len({x['id'] for x in rows}) != 142:
        raise ValueError('closed count/unique IDs')
    for row in rows:
        if set(row) != {'id', 'source', 'mode', 'expected'} or row['mode'] not in ('return', 'throw') or type(row['id']) is not str or type(row['source']) is not str:
            raise ValueError('case schema')
        if len(Scanner(row['source']).all()) != 1 or set(row['expected']) != {'value', 'trace'}:
            raise ValueError('complete source/result')
        observation(row['expected']['value'])
        observation(row['expected']['trace'])
        if row['expected']['trace']['tag'] != 'number':
            raise ValueError('trace number')
    return rows

JS = '''(function decode(v,B,R,D,depth,seen){
if(depth>64)throw Error('depth');
if(v===null)return {tag:'nil'};
if(typeof v==='string')return {tag:'string',units:Array.from({length:v.length},function(_,i){return v.charCodeAt(i)})};
if(typeof v==='boolean')return {tag:'bool',value:v};
if(typeof v==='number'){var b=new ArrayBuffer(8),d=new DataView(b);d.setFloat64(0,v,false);return {tag:'number',bits:Array.from(new Uint8Array(b),function(x){return x.toString(16).padStart(2,'0')}).join('')}}
if(seen.indexOf(v)>=0)throw Error('cycle');
var path=seen.concat([v]), rec=function(x){return decode(x,B,R,D,depth+1,path)};
if(v instanceof cljs.core.PersistentVector){
 if(!Number.isSafeInteger(v.cnt)||v.cnt<0||v.cnt>4096)throw Error('vector bound');
 var values=[],tailoff=v.cnt<32?0:((v.cnt-1)>>>5)<<5;
 for(var i=0;i<v.cnt;i++){var a;
 if(i>=tailoff)a=v.tail;
 else {var n=v.root;for(var level=v.shift;level>0;level-=5)n=n.arr[(i>>>level)&31];a=n.arr;}
 values.push(rec(a[i&31]));}
 return {tag:'vector',items:values};}

if(v instanceof B||v instanceof R)return {tag:'node',class:v instanceof B?'BlackNode':'RedNode',key:rec(v.key),val:rec(v.val),left:rec(v.left),right:rec(v.right),hash:rec(v.__hash)};
if(v instanceof D)return {tag:'reduced',value:rec(v.val)};
if(v instanceof Error&&v.constructor===Error)return {tag:'error',class:'Error',message:Array.from({length:v.message.length},function(_,i){return v.message.charCodeAt(i)})};
throw Error('unsupported raw value');})(~{},~{},~{},~{},0,[])'''

def driver():
    header = '(ns suss-oracle.sorted-tree)\n' + ''.join('(def '+x+' cljs.core/'+x+')\n' for x in ['BlackNode','RedNode','MapEntry','balance-left','balance-right','balance-left-del','balance-right-del','tree-map-kv-reduce','nil-iter','seq-iter','iter','es6-iterator','es6-entries-iterator','es6-set-entries-iterator'])
    header += '(def sorted-effects 0)\n(defn decode-raw [v] (js* '+json.dumps(JS)+' v cljs.core/BlackNode cljs.core/RedNode cljs.core/Reduced))\n'
    rows = []
    for c in cases():
        rows.append('(do (set! sorted-effects 0) (let [result (try #js {:mode "return" :value (decode-raw '+c['source']+')} (catch :default e #js {:mode "throw" :value (decode-raw e)}))] #js {:id '+json.dumps(c['id'])+' :mode (.-mode result) :value (.-value result) :trace (decode-raw sorted-effects)}))')
    return header + '(defn -main [] (println (.stringify js/JSON #js [\n'+'\n'.join(rows)+'\n])))\n(set! *main-cli-fn* -main)\n'

def generate():
    (ROOT / 'tests/oracle/src/suss_oracle/sorted_tree.cljs').write_text(driver())

def check():
    if (ROOT / 'tests/oracle/src/suss_oracle/sorted_tree.cljs').read_text() != driver():
        raise ValueError('stale driver')
    print('PASS 142 closed sorted tree cases/driver; not executed')

def compare(path=None):
    actual = read(path or ROOT / 'tests/oracle/out/sorted-tree-observations.json')
    expected = [dict(id=c['id'], mode=c['mode'], **c['expected']) for c in cases()]
    if not isinstance(actual, list) or len(actual) != len(expected):
        raise ValueError('observation count')
    for a, e in zip(actual, expected):
        if not isinstance(a, dict) or set(a) != {'id', 'mode', 'value', 'trace'}:
            raise ValueError('result schema')
        observation(a['value']); observation(a['trace'])
        if a != e:
            raise ValueError('raw mismatch '+e['id']+': '+json.dumps(a))
    print('PASS 142 independent pinned sorted tree observations')

if __name__ == '__main__':
    {'generate': generate, 'check': check, 'compare': compare}[sys.argv[1]]()
