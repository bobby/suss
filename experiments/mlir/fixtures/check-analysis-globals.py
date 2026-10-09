#!/usr/bin/env python3
# Synthetic schema/storage regressions, not genuine native source observations.
import copy,json,re,subprocess,sys
from pathlib import Path
from runpy import run_path
P=Path(__file__).parent;sh=run_path(str(P/'analysis-shapes.py'));node=sh['node'];form=sh['form'];symbol=sh['symbol'];envelope=sh['envelope'];literal=sh['literal']
def global_resolution(name,decl=None):return dict(tag='global',global_=None) if False else {'tag':'global','global':{'phase':'runtime','namespace':'suss.core','name':name},'declaration':{'present':False} if decl is None else {'present':True,'value':decl}}
def source_list(*items):return form({'tag':'list','items':list(items)})
info=dict(definitionForm=source_list(symbol('def'),symbol('x'),literal()['originalForm']),analysisCompleted=True,declaration=symbol('x'),docstring={'present':True,'value':[55296,65]},origin={'present':True,'value':{'sourceBytes':100,'path':{'present':True,'value':[65,55296]}}},initializerForm={'present':True,'value':literal()['originalForm']},initializerPresent=True,typeFields={'present':True,'value':0},once=False)
g=envelope(node('global',symbol('x'),resolved=global_resolution('x',info)))
let=envelope(node('let',source_list(symbol('let'),form({'tag':'vector','items':[]}),literal()['originalForm']),resolved=global_resolution('let'),children=[{'role':'body','node':literal()}]))
b=sh['declaration']('f',0,0,literal());visible=sh['localmap'](b);callee=sh['local'](b,visible)
invoke=envelope(node('invoke',source_list(symbol('f'),literal()['originalForm']),locals=visible,resolved=copy.deepcopy(callee['resolved']),children=[{'role':'callee','node':callee},{'role':'argument','node':literal(locals=visible)}]))
positives={'global-declaration':g,'global-head':let,'local-invoke-head':invoke}
mutations={}
def bad(name,base,change):v=copy.deepcopy(base);change(v['selectedFacts']);mutations[name]=v
bad('global-extra',g,lambda n:n['resolved']['global'].update(extra=True))
bad('global-name',g,lambda n:n['resolved']['global'].update(name='y'))
bad('global-phase',g,lambda n:n['resolved']['global'].update(phase='macro'))
bad('global-declaration-name',g,lambda n:n['resolved']['declaration']['value'].update(declaration=symbol('y')))
bad('global-declaration-extra',g,lambda n:n['resolved']['declaration']['value'].update(extra=True))
bad('global-origin-extra',g,lambda n:n['resolved']['declaration']['value']['origin']['value'].update(extra=True))
bad('global-origin-size',g,lambda n:n['resolved']['declaration']['value']['origin']['value'].update(sourceBytes=-1))
bad('global-path-unit',g,lambda n:n['resolved']['declaration']['value']['origin']['value']['path'].update(value=[65536]))
bad('global-docstring-unit',g,lambda n:n['resolved']['declaration']['value']['docstring'].update(value=[-1]))
bad('global-type-fields',g,lambda n:n['resolved']['declaration']['value']['typeFields'].update(value=-1))
bad('global-initializer-presence',g,lambda n:n['resolved']['declaration']['value'].update(initializerPresent='true'))
bad('global-absent-extra',let,lambda n:n['resolved']['declaration'].update(value=None))
bad('global-on-literal',g,lambda n:n.update(op='literal',originalForm=literal()['originalForm']))
bad('global-on-vector',g,lambda n:n.update(op='vector',originalForm=form({'tag':'vector','items':[]})))
bad('field-still-unsupported',g,lambda n:n.update(resolved={'tag':'field'}))
bad('invoke-head-name',invoke,lambda n:n.update(originalForm=source_list(symbol('other'),literal()['originalForm'])))
bad('invoke-callee-resolution',invoke,lambda n:n['children'][0]['node'].update(resolved=None))
(P/'analysis-global').mkdir(exist_ok=True)
def mlir(v):return 'module attributes {suss.source_analysis = '+sh['mlir_string'](v)+'} {}\n'
def run(text):return subprocess.run([sys.argv[1]],input=text,capture_output=True,text=True,timeout=20)
for name,v in positives.items():
 text=mlir(v);(P/'analysis-global'/('positive-'+name+'.mlir')).write_text(text)
 r=run(text);assert r.returncode==0,(name,r.stderr)
 r2=run(r.stdout);assert r2.returncode==0 and r2.stdout==r.stdout,(name,r2.stderr)
 # Recover the entire original payload, not a projection or inferred SSA facts.
 raw=re.search(r'suss.source_analysis\s*=\s*"((?:\\.|[^"\\])*)"',r.stdout)[1]
 out=bytearray();i=0
 while i<len(raw):
  if raw[i]=='\\':out.append(int(raw[i+1:i+3],16));i+=3
  else:out.extend(raw[i].encode());i+=1
 assert json.loads(out)==v,name
for name,v in mutations.items():
 text=mlir(v);(P/'analysis-global'/('negative-'+name+'.mlir')).write_text(text);r=run(text);assert r.returncode!=0,(name,r.stdout)
print(f'PASS: {len(positives)} global/head positives full roundtrip, {len(mutations)} strict negatives (synthetic; native pending)')
