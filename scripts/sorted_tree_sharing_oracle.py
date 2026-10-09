"""Exact independent host shapes/reference facts; native execution pending."""
import json
from pathlib import Path
from tree_node_oracle import unique
ROOT=Path(__file__).resolve().parents[1]
def node(cls,k,v,left=None,right=None):
    return {'class':cls,'key':k,'val':v,'left':left,'right':right,'hash':None}
def expected():
    leaf=node('RedNode',1,10)
    return {'old':node('BlackNode',2,20,leaf,node('RedNode',3,30)),
            'updated':node('BlackNode',3,30,node('BlackNode',2,20,leaf),node('BlackNode',4,40)),
            'rootChanged':True,'directLeftShared':False,'nestedLeftShared':True,'comparatorShared':True}
def validate(actual):
    def shape(v, depth=0):
        if depth>64:raise ValueError("node depth")
        if v is None:return
        if not isinstance(v,dict) or set(v)!={'class','key','val','left','right','hash'} or v['class'] not in ('BlackNode','RedNode') or type(v['key']) is not int or type(v['val']) is not int or v['hash'] is not None:raise ValueError('raw node schema/type')
        shape(v['left'], depth+1);shape(v['right'], depth+1)
    if not isinstance(actual,dict) or set(actual)!=set(expected()):raise ValueError('sharing schema')
    shape(actual['old']);shape(actual['updated'])
    for k in ['rootChanged','directLeftShared','nestedLeftShared','comparatorShared']:
        if type(actual[k]) is not bool:raise ValueError('reference Boolean type')
    if actual!=expected():raise ValueError('rotation shape/reference mismatch')
def check(path):
    validate(json.loads(path.read_text(),object_pairs_hook=unique))
    print('PASS exact old/new rotation shapes and four reference observations')
if __name__=='__main__':check(ROOT/'tests/oracle/out/sorted-tree-sharing.json')
