#!/usr/bin/env python3
"""Synthetic draft-schema shape witnesses, NOT generated/native HIR evidence.
Build the independently expected complete payloads for MLIR storage tests.
Original Suss MIT/Apache-2.0 code.
"""
import json

SCHEMA = 'suss.source-analysis.draft.v1'
def form(data, span=(0, 0), metadata=None):
    return dict(span=list(span), metadata=metadata or [], data=data)
def symbol(name):
    return form(dict(tag='symbol', namespace=None, name=name), (1, 2))
def node(op, original=None, locals=None, **fields):
    value = dict(op=op, originalForm=original or form(dict(tag='nil')),
        span=[0, 0], metadata=[], physicalType=dict(tag='value'), isBody=False,
        context='expression', phase='runtime', namespace='user', scopeNamespace='user',
        snapshotNamespace='user', tag=dict(present=False), inferred=dict(present=False),
        quotedConstTag=dict(present=False), inferredReturn=dict(present=False),
        resolved=None, locals=locals or [], declarations=[], children=[], callable=None, captures=[])
    value.update(fields)
    return value

def literal(bits='401c000000000000', locals=None):
    return node('literal', form(dict(tag='f64', bits=bits), (3, 4)), locals,
        span=[3, 4], physicalType=dict(tag='number'))
def declaration(name, ordinal, hir, initializer=None, argument=None, shadow=None):
    return dict(bindingId='binding:'+str(ordinal), hirBindingId=hir,
        physicalType=dict(tag='number'), declaration=symbol(name),
        kind=dict(tag='let') if argument is None else dict(tag='argument', index=argument, rest=False),
        context='expression', initializer=initializer, shadow=shadow)
def localmap(*bindings):
    return sorted([dict(name=b['declaration']['data']['name'], binding=b) for b in bindings], key=lambda x:x['name'])
def local(b, visible):
    return node('local', symbol(b['declaration']['data']['name']), visible,
        physicalType=b['physicalType'], resolved=dict(tag='local', binding=b))
def envelope(selected):
    return dict(schema=SCHEMA, selectedFacts=selected)

def witnesses():
    x = declaration('x', 0, 41, literal())
    y = declaration('y', 1, 52, argument=0)
    body = node('do', form(dict(tag='list', items=[])), localmap(x,y), isBody=True,
        children=[dict(role='result', node=local(y, localmap(x,y)))])
    closure = node('closure', form(dict(tag='list', items=[])), localmap(x),
        physicalType=dict(tag='closure', arity=1),
        captures=[dict(bindingId=x['bindingId'], hirBindingId=x['hirBindingId'])],
        callable=dict(form=form(dict(tag='list', items=[])), declarations=[y],
            parameters=[dict(hirBindingId=y['hirBindingId'], name='y', span=y['declaration']['span'], metadata=[])],
            variadic=False, recurs=dict(present=True, value=False), entryLocals=localmap(x), entryContext='expression', body=body))
    f = declaration('f', 2, 73, closure)
    f['physicalType'] = dict(tag='closure', arity=1)
    shadow = declaration('x', 3, 89, literal('4022000000000000', localmap(x,f)), shadow=x)
    visible = localmap(f, shadow)
    false_form = form(dict(tag='bool', value=False), (7, 8))
    nil_form = form(dict(tag='nil'), (8, 9))
    units_form = form(dict(tag='utf16', units=[0xd800,0,0xdc00]), (9, 10))
    vector = node('vector', form(dict(tag='vector', items=[false_form,nil_form,units_form])), visible,
        children=[dict(role='item',node=node('literal',f0,visible)) for f0 in [false_form,nil_form,units_form]])
    invoke = node('invoke', form(dict(tag='list',items=[])), visible,
        children=[dict(role='callee',node=local(f,visible)),dict(role='argument',node=local(shadow,visible))])
    result = node('do', form(dict(tag='list',items=[])), visible, isBody=True,
        children=[dict(role='statement',node=vector),dict(role='result',node=invoke)])
    inner = node('let', form(dict(tag='list',items=[])), localmap(x,f), declarations=[shadow],
        children=[dict(role='init',bindingId=shadow['bindingId'],node=shadow['initializer']),dict(role='body',node=result)])
    selected = node('let', form(dict(tag='list',items=[])), declarations=[x,f],
        metadata=[units_form,false_form,nil_form], tag=dict(present=True,value=symbol('number')),
        quotedConstTag=dict(present=True,value=None), inferredReturn=dict(present=True,value=nil_form),
        children=[dict(role='init',bindingId=x['bindingId'],node=x['initializer']),dict(role='init',bindingId=f['bindingId'],node=closure),dict(role='body',node=inner)])
    return envelope(selected), envelope(literal()), envelope(closure)

def mlir_string(value):
    return json.dumps(json.dumps(value, ensure_ascii=True, separators=(',', ':')))

if __name__ == '__main__':
    from pathlib import Path
    root = Path(__file__).parent
    sidecar, scalar, closure = witnesses()
    (root/'positive-analysis-source.mlir').write_text(
        '// Synthetic schema/storage witness only; NOT native HIR evidence.\n'
        '// Original Suss MIT/Apache-2.0 fixture.\n'
        'module attributes {suss.source_analysis = '+mlir_string(sidecar)+'} {\n'
        ' %x = "suss.const"() {value = 7.0 : f64, suss.analysis = '+mlir_string(scalar)+'} : () -> f64 loc("source.sus":3:4)\n'
        ' %f = "suss.closure"(%x) ({ ^entry(%c: f64, %y: f64): "suss.return"(%y) : (f64) -> () }) {suss.analysis = '+mlir_string(closure)+'} : (f64) -> !suss.closure<(f64) -> f64, [f64]> loc("source.sus":5:6)\n}\n')
    def negative(name, expected, value=None, raw=None, attach=None):
        encoded = json.dumps(raw) if raw is not None else mlir_string(value)
        if attach is None:
            body='module attributes {suss.source_analysis = '+encoded+'} {}'
        elif attach=='add':
            body='module { %x = "suss.const"() {value = 1.0 : f64} : () -> f64 %r = "suss.add"(%x,%x) {suss.analysis = '+encoded+'} : (f64,f64) -> f64 }'
        else:
            body='module { %x = "suss.const"() {value = 1.0 : f64, suss.analysis = '+encoded+'} : () -> f64 }'
        (root/('negative-analysis-'+name+'.mlir')).write_text('// expected: '+expected+'\n// Synthetic malformed schema witness, MIT/Apache-2.0.\n'+body+'\n')
    def changed(name, expected, mutate):
        value=json.loads(json.dumps(sidecar)); mutate(value); negative(name,expected,value)
    changed('version','unsupported schema version',lambda v:v.update(schema='suss.source-analysis.v0'))
    changed('unknown-field','missing/unknown object fields',lambda v:v['selectedFacts'].update(extra=0))
    changed('missing-field','missing/unknown object fields',lambda v:v['selectedFacts'].pop('metadata'))
    changed('span','reversed span',lambda v:v['selectedFacts'].update(span=[9,1]))
    changed('utf16','UTF16 unit out of range',lambda v:v['selectedFacts']['metadata'][0]['data'].update(units=[65536]))
    changed('optional','missing/unknown object fields',lambda v:v['selectedFacts'].update(inferred={'present':False,'value':None}))
    changed('bits','f64 requires exact lowercase bits',lambda v:v['selectedFacts']['declarations'][0]['initializer']['originalForm']['data'].update(bits='7.0'))
    changed('source-op','unsupported tag/context/phase',lambda v:v['selectedFacts'].update(op='suss.add'))
    changed('child-reference','dangling/reordered initializer reference',lambda v:v['selectedFacts']['children'][0].update(bindingId='binding:999'))
    changed('child-order','dangling/reordered initializer reference',lambda v:v['selectedFacts']['children'].reverse())
    # Standalone closure avoids changing its repeated copies in the whole sidecar.
    value=json.loads(json.dumps(closure)); value['selectedFacts']['captures'][0]['bindingId']='binding:999'
    negative('capture-reference','dangling/duplicate capture reference',value)
    value=json.loads(json.dumps(closure)); value['selectedFacts']['locals'][0]['name']='wrong'
    negative('local-name','local name/declaration mismatch',value)
    value=json.loads(json.dumps(closure)); value['selectedFacts']['callable']['parameters'][0]['hirBindingId']=100
    negative('parameter-identity','parameter facts disagree',value)
    value=json.loads(json.dumps(closure)); value['selectedFacts']['callable']['body']['locals'][0]['binding']['physicalType']={'tag':'value'}
    negative('conflicting-binding','conflicting declaration identity',value)
    value=json.loads(json.dumps(closure)); value['selectedFacts']['callable']['declarations'][0]['hirBindingId']=41
    negative('conflicting-hir-id','conflicting hirBindingId',value)
    value=json.loads(json.dumps(sidecar));n=value['selectedFacts']
    n['declarations'].insert(1,n['declarations'][0]);n['children'].insert(1,n['children'][0])
    negative('duplicate-declaration','duplicate source declaration',value)
    value=json.loads(json.dumps(closure));c=value['selectedFacts']['callable']
    c['declarations']=[];c['parameters']=[]
    negative('unbound-method-argument','callable body scope disagrees with declarations',value)
    value=json.loads(json.dumps(closure));value['selectedFacts']['callable']['body']['children'][0]['node']['originalForm']['data']['name']='x'
    negative('resolved-symbol-name','local symbol/resolution mismatch',value)
    value=json.loads(json.dumps(scalar));value['selectedFacts']['locals']=closure['selectedFacts']['locals']
    value['selectedFacts']['resolved']={'tag':'local','binding':closure['selectedFacts']['locals'][0]['binding']}
    negative('resolved-literal','resolution on nonlocal node',value)
    value=json.loads(json.dumps(closure));value['selectedFacts']['physicalType']['arity']=2
    negative('closure-arity','closure arity disagrees with callable',value)
    value=json.loads(json.dumps(closure));n=value['selectedFacts'];c=n['callable']
    c['declarations']=[];c['parameters']=[];c['body']['locals']=n['locals'];n['physicalType']['arity']=0
    negative('hidden-unbound-argument','child scope disagrees with declarations',value)
    value=json.loads(json.dumps(closure));value['selectedFacts']['callable']['entryLocals']=[]
    negative('method-entry-scope','callable body scope disagrees with declarations',value)
    value=json.loads(json.dumps(sidecar));value['selectedFacts']['children'][-1]['node']['declarations'][0]['shadow']=None
    # Keep initializer child correspondence intact; repeated facts elsewhere must
    # reject the changed declaration or lexical shadow, whichever is seen first.
    negative('shadow-scope','shadow disagrees with lexical scope',value)
    raw=json.dumps(scalar,separators=(',',':'))
    negative('duplicate-key','duplicate keys',raw=raw[:-1]+',"schema":"'+SCHEMA+'"}')
    negative('escaped-duplicate-key','duplicate keys',raw=raw[:-1]+',"schem\\u0061":"'+SCHEMA+'"}')
    negative('json','invalid JSON',raw='{')
    negative('unrelated-attachment','unsupported operation/source attachment mapping',scalar,attach='add')
    value=json.loads(json.dumps(scalar));value['selectedFacts']['originalForm']['data']={'tag':'nil'}
    negative('non-f64-const','unsupported operation/source attachment mapping',value,attach='const')
    (root/'negative-analysis-legacy.mlir').write_text('// expected: legacy dictionaries unsupported\nmodule { %x = "suss.const"() {value = 1.0 : f64, suss.analysis = {binding = 0 : i64, children = [], metadata = []}} : () -> f64 }\n')
