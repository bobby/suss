"""Pinned Error constructor observations; native compilation/execution pending."""
import json
import sys
from pathlib import Path
from tree_node_oracle import unique
from sorted_tree_oracle import observation as scalar_observation
from cljs_inventory import Scanner
ROOT=Path(__file__).resolve().parents[1]
PIN='c4295f303100bbf5afac449242d30bca1126f1a1'
COUNT=151

def read(path):return json.loads(path.read_text(),object_pairs_hook=unique)
def observation(v,depth=0):
    if depth>64:raise ValueError('depth')
    if not isinstance(v,dict):raise ValueError('value schema')
    if v.get('tag')=='undefined':
        if set(v)!={'tag'}:raise ValueError('undefined schema')
    elif v.get('tag')=='data-descriptor':
        if set(v)!={'tag','value','writable','enumerable','configurable'} or any(type(v[k]) is not bool for k in ['writable','enumerable','configurable']):raise ValueError('descriptor schema')
        observation(v['value'],depth+1)
    elif v.get('tag')=='error':
        if set(v)!={'tag','class','message','messageOwn','causeOwn','cause'} or v['class'] not in ['Error','RangeError','TypeError'] or type(v['messageOwn']) is not bool or type(v['causeOwn']) is not bool:raise ValueError('Error schema/class/presence')
        scalar_observation({'tag':'string','units':v['message']})
        observation(v['cause'],depth+1)
        if not v['causeOwn'] and v['cause']!={'tag':'undefined'}:raise ValueError('absent cause value')
    else:
        scalar_observation(v,depth)
        if v['tag'] not in ('nil','bool','string','number','vector'):raise ValueError('unsupported scalar')

def cases():
    c=read(ROOT/'tests/oracle/error-constructor-cases.json')
    if set(c)!={'schema','upstream','cases'} or type(c['schema']) is not int or c['schema']!=1 or c['upstream']!=PIN:raise ValueError('pin/schema')
    rows=c['cases']
    if not isinstance(rows,list) or len(rows)!=COUNT or len({v['id'] for v in rows})!=COUNT:raise ValueError('closed IDs/count')
    for c in rows:
        if set(c)!={'id','source','mode','expected'} or type(c['id']) is not str or type(c['source']) is not str or c['mode'] not in ('return','throw') or len(Scanner(c['source']).all())!=1 or set(c['expected'])!={'value','trace'}:raise ValueError('case schema/balance')
        observation(c['expected']['value']);observation(c['expected']['trace'])
        if c['expected']['trace']['tag']!='number':raise ValueError('trace')
    return rows
JS='''(function decode(v,depth){
if(depth>64)throw Error('depth');
if(v===undefined)return {tag:'undefined'};
if(v===null)return {tag:'nil'};
if(typeof v==='boolean')return {tag:'bool',value:v};
if(typeof v==='string')return {tag:'string',units:Array.from({length:v.length},function(_,i){return v.charCodeAt(i)})};
if(typeof v==='number'){var b=new ArrayBuffer(8),d=new DataView(b);d.setFloat64(0,v,false);return {tag:'number',bits:Array.from(new Uint8Array(b),function(x){return x.toString(16).padStart(2,'0')}).join('')}}
if(v&&Object.getPrototypeOf(v)===Object.prototype&&Object.keys(v).sort().join(',')==='configurable,enumerable,value,writable'){
 if(typeof v.writable!=='boolean'||typeof v.enumerable!=='boolean'||typeof v.configurable!=='boolean')throw Error('descriptor flags');
 return {tag:'data-descriptor',value:decode(v.value,depth+1),writable:v.writable,enumerable:v.enumerable,configurable:v.configurable};
}
if(v instanceof Error&&(v.constructor===Error||v.constructor===RangeError||v.constructor===TypeError)){
 var msg=Object.getOwnPropertyDescriptor(v,'message'),cause=Object.getOwnPropertyDescriptor(v,'cause');
 if(msg&&(!Object.prototype.hasOwnProperty.call(msg,'value')||typeof msg.value!=='string'))throw Error('message storage');
 if(cause&&!Object.prototype.hasOwnProperty.call(cause,'value'))throw Error('cause storage');
 var s=msg?msg.value:'';
 return {tag:'error',class:v.constructor===Error?'Error':v.constructor===RangeError?'RangeError':'TypeError',message:Array.from({length:s.length},function(_,i){return s.charCodeAt(i)}),messageOwn:!!msg,causeOwn:!!cause,cause:decode(cause?cause.value:undefined,depth+1)};
}
throw Error('unsupported raw Error observation');})(~{},0)'''
def driver():
    header='(ns suss-oracle.error-constructors)\n(def error-effects 0)\n(defn error-step [n] (set! error-effects (+ (* error-effects 10) n)))\n(defn decode-raw [v] (js* '+json.dumps(JS)+' v))\n'
    rows=[]
    for c in cases():
        rows.append('(do (set! error-effects 0) (let [r (try #js {:mode "return" :value (decode-raw '+c['source']+')} (catch :default e #js {:mode "throw" :value (decode-raw e)}))] #js {:id '+json.dumps(c['id'])+' :mode (.-mode r) :value (.-value r) :trace (decode-raw error-effects)}))')
    return header+'(defn -main [] (println (.stringify js/JSON #js [\n'+'\n'.join(rows)+'\n])))\n(set! *main-cli-fn* -main)\n'
def check():
    if (ROOT/'tests/oracle/src/suss_oracle/error_constructors.cljs').read_text()!=driver():raise ValueError('driver')
    print('PASS 151 closed Error cases/driver; native pending')
def compare(path=None):
    actual=read(path or ROOT/'tests/oracle/out/error-constructors.json')
    expected=[dict(id=c['id'],mode=c['mode'],**c['expected']) for c in cases()]
    if not isinstance(actual,list) or len(actual)!=COUNT:raise ValueError('ordered observation count')
    for a,e in zip(actual,expected):
        if not isinstance(a,dict) or set(a)!={'id','mode','value','trace'}:raise ValueError('result schema')
        observation(a['value']);observation(a['trace'])
        if a!=e:raise ValueError('Error observation mismatch '+e['id']+'\nactual='+json.dumps(a)+'\nexpected='+json.dumps(e))
    print('PASS 151 exact pinned Error constructor/ancestry/effect observations')
if __name__=='__main__':
    if sys.argv[1]=='generate':(ROOT/'tests/oracle/src/suss_oracle/error_constructors.cljs').write_text(driver())
    elif sys.argv[1]=='check':check()
    elif sys.argv[1]=='compare':compare()
    else:raise ValueError('command')
