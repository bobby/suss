#!/usr/bin/env python3
"""Genuine source facts through explicit numeric v2 export; optional actual execution.
The numeric graph is hand-authored, not an automatic source-to-MLIR compiler.
"""
import argparse
import json
from pathlib import Path
import subprocess
import tempfile
import copy
from source_correspondence import verify
P=Path(__file__).parent
parser=argparse.ArgumentParser();parser.add_argument('bridge');parser.add_argument('exporter');parser.add_argument('--execute',action='store_true');args=parser.parse_args()
def run(command):
    r=subprocess.run(command,capture_output=True,text=True,timeout=60)
    assert r.returncode==0,r.stderr.splitlines()[0] if r.stderr else r.returncode
    return r.stdout
def encode(facts):
    return '"'+''.join('\\'+format(b,'02X') for b in json.dumps(facts,separators=(',',':')).encode())+'"'
base=(P.parent/'export-fixtures/producer-caller.mlir').read_text()
source=(P/'closure.sus').read_text()
with tempfile.TemporaryDirectory(prefix='suss-source-export-') as directory:
    d=Path(directory);sentinel=d/'sentinel.json'
    for changed,expected_bits in [(False,'4022000000000000'),(True,'402a000000000000')]:
        native_source=d/'source.sus';native_source.write_text(source.replace('x 7','x 11') if changed else source)
        facts=json.loads(run([args.bridge,'--analyze-source',str(native_source)]))
        mlir=base.replace('value = 7.0','value = 11.0') if changed else base
        mlir=mlir.replace('module {','module attributes {suss.source_analysis = '+encode(facts)+'} {',1)
        input_file=d/'input.mlir';input_file.write_text(mlir)
        graph=json.loads(run([args.exporter,str(input_file),'--expected-bits='+expected_bits,'--preserve-source-analysis']))
        assert graph['schema']=='suss.mlir.bridge.source.v2' and json.loads(graph['source_analysis'])==facts
        mapping=verify(facts,graph)
        assert mapping['literal_bits']==('4026000000000000' if changed else '401c000000000000')
        for corrupt in ('literal','operand','capture','argument','bool-capture','bool-operand','float-return'):
            wrong_graph=copy.deepcopy(graph)
            if corrupt=='literal': wrong_graph['producer']['operations'][0]['bits']='4031000000000000'
            elif corrupt=='operand': wrong_graph['producer']['operations'][1]['body']['operations'][0]['lhs']=1
            elif corrupt=='capture': wrong_graph['producer']['operations'][1]['captures']=[1]
            elif corrupt=='argument': wrong_graph['caller']['operations'][1]['arguments']=[0]
            elif corrupt=='bool-capture': wrong_graph['producer']['operations'][1]['captures']=[False]
            elif corrupt=='bool-operand': wrong_graph['producer']['operations'][1]['body']['operations'][0]['lhs']=False
            else: wrong_graph['producer']['operations'][1]['body']['return_value']=2.0
            try: verify(facts,wrong_graph)
            except ValueError: pass
            else: raise AssertionError('source correspondence accepted '+corrupt)
        retained=json.loads((P.parent/'export-fixtures'/('exported-mutated.json' if changed else 'exported-original.json')).read_text())
        projected={k:v for k,v in graph.items() if k!='source_analysis'};projected['schema']='suss.mlir.bridge.v1'
        assert projected==retained,'source carrier changed executable numeric graph'
        exported=d/'graph.json';exported.write_text(json.dumps(graph))
        if args.execute:
            out=run([args.bridge,'--source-graph',str(exported),str(native_source)])
            assert 'source_facts_match_genuine_analysis=true' in out and 'source_facts_preserved_in_both_wasm_fragments=true' in out and 'actual_bits='+expected_bits in out
            wrong=d/'wrong.sus';wrong.write_text(source.replace('x 7','x 17'))
            r=subprocess.run([args.bridge,'--source-graph',str(exported),str(wrong)],capture_output=True,text=True,timeout=60)
            assert r.returncode!=0,'mismatched original facts accepted'
            assert 'complete selected source facts differ' in r.stderr, r.stderr
            assert 'source_facts_match_genuine_analysis=true' not in r.stdout and 'actual_bits=' not in r.stdout
        # The v1 route still rejects a source attribute, leaving output untouched.
        sentinel.write_text('sentinel\n')
        r=subprocess.run([args.exporter,str(input_file),'--expected-bits='+expected_bits,'--export-graph='+str(sentinel)],capture_output=True,text=True,timeout=60)
        assert r.returncode!=0 and sentinel.read_text()=='sentinel\n'
        malformed=json.loads(json.dumps(facts));malformed['extra']=False
        bad=d/'bad.mlir';bad.write_text(mlir.replace(encode(facts),encode(malformed)))
        r=subprocess.run([args.exporter,str(bad),'--expected-bits='+expected_bits,'--preserve-source-analysis','--export-graph='+str(sentinel)],capture_output=True,text=True,timeout=60)
        assert r.returncode!=0 and sentinel.read_text()=='sentinel\n'
    plain=d/'plain.mlir';plain.write_text(base)
    r=subprocess.run([args.exporter,str(plain),'--expected-bits=4022000000000000','--preserve-source-analysis','--export-graph='+str(sentinel)],capture_output=True,text=True,timeout=60)
    assert r.returncode!=0 and sentinel.read_text()=='sentinel\n'
print('PASS genuine original/mutated source facts and unchanged numeric graphs; v1/source-malformation/missing-carrier fail closed')
print('actual two-fragment WasmGC source preservation: '+('PASS' if args.execute else 'NOT RUN'))
