#!/usr/bin/env python3
"""Compare complete synthetic source-facts payloads across MLIR print/parse.
This is storage/schema validation evidence, not native source analysis execution.
"""
import json
from pathlib import Path
import re
import subprocess
import sys
from runpy import run_path
root=Path(__file__).parent
shapes=run_path(str(root/'analysis-shapes.py'))
expected=shapes['witnesses']()
source=(root/'positive-analysis-source.mlir').read_text()

def run(source):
    p=subprocess.run([sys.argv[1],'--mlir-print-debuginfo'],input=source,text=True,capture_output=True,timeout=20)
    assert p.returncode==0,p.stderr
    return p.stdout

def unescape(raw):
    out=bytearray();i=0
    while i<len(raw):
        if raw[i]!='\\': out.extend(raw[i].encode('utf8'));i+=1;continue
        i+=1
        if i+1<len(raw) and all(c in '0123456789abcdefABCDEF' for c in raw[i:i+2]):
            out.append(int(raw[i:i+2],16));i+=2
        else:
            out.extend({'n':b'\n','r':b'\r','t':b'\t','"':b'"','\\':b'\\'}[raw[i]]);i+=1
    return out.decode('utf8')
first=run(source);second=run(first)
assert first==second,'unstable source-facts print/parse'
for output in [source,first,second]:
    payloads=[json.loads(unescape(x)) for x in re.findall(r'suss\.(?:source_analysis|analysis)\s*=\s*"((?:\\.|[^"\\])*)"',output)]
    assert len(payloads)==3,'lost/misattached source payload'
    assert payloads==list(expected),'complete selected source facts changed'
    assert output.index('suss.source_analysis')<output.index('"suss.const"')<output.index('"suss.closure"')
for loc in ['"source.sus":3:4','"source.sus":5:6']: assert loc in first,'lost attached source location'
# Valid metadata and source-child mutations remain valid source observations,
# but must differ from the independently expected complete storage witness.
for old,new in [('55296','55297'),('"inferred":{"present":false}','"inferred":{"present":true,"value":{"span":[0,0],"metadata":[],"data":{"tag":"nil"}}}')]:
    modified=json.loads(json.dumps(expected[0],separators=(',',':')).replace(old,new,1))
    assert modified!=expected[0],'mutation did not change expected facts'
    encoded=shapes['mlir_string'](modified)
    printed=run('module attributes {suss.source_analysis = '+encoded+'} {}')
    received=json.loads(unescape(re.search(r'suss\.source_analysis\s*=\s*"((?:\\.|[^"\\])*)"',printed)[1]))
    assert received==modified and received!=expected[0],'source mutation lost or treated as original'
print('PASS complete selected-facts fields/attachments/locations and metadata/presence mutation storage (synthetic, not native HIR)')

# Bounded malformed-input checks; no unbounded external parser invocations.
raws = [('{"schema":"'+shapes['SCHEMA']+'","selectedFacts":{},"padding":"'+'x'*(1<<20)+'"}', 'exceeds 1 MiB'),
        ('['*140+'0'+']'*140, 'nesting limit')]
for raw, diagnostic in raws:
    p=subprocess.run([sys.argv[1]], input='module attributes {suss.source_analysis = '+json.dumps(raw)+'} {}', text=True, capture_output=True, timeout=20)
    assert p.returncode==1 and diagnostic in p.stderr, (p.returncode,p.stderr[:500])
print('PASS source-analysis byte/nesting bounds')
