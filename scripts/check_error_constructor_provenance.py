"""Verify complete licensed Error parents and bounded authored evidence, not native support."""
import hashlib
import json
from pathlib import Path
from core_import import extract_forms, pinned_file
from cljs_inventory import Scanner
from error_constructor_oracle import PIN, check, compare, read
ROOT=Path(__file__).resolve().parents[1]
FOLDER=ROOT/'docs/compatibility/error-constructors'
def digest(data):return hashlib.sha256(data).hexdigest()
def verify():
    provenance=read(FOLDER/'provenance.json')
    assert provenance['upstream']==PIN
    for row in provenance['whole-original-parents']:
        path=row['source'].removeprefix('clojurescript/')
        source=pinned_file(ROOT,path).decode()
        if row['id'].startswith('anonymous:'):
            forms=[f for f in Scanner(source).all() if source.count('\n',0,f.start)+1==row['line']]
            assert len(forms)==1
            f=forms[0];text=source[f.start:f.end]
            assert source.count('\n',0,f.end)+1==row['end-line']
        else:
            phase='macro' if row['id'].startswith('macro:') else 'runtime'
            form=extract_forms(source,phase,path)[row['id']]
            assert (form['line'],form['end-line'])==(row['line'],row['end-line'])
            text=form['form']
        assert digest(text.encode())==row['sha256']
        retained=(ROOT/row['retained']).read_bytes()
        assert retained==(text+'\n').encode()
        assert digest(retained)==row['retained-sha256']
    assert (FOLDER/'epl-v10.html').read_bytes()==pinned_file(ROOT,'epl-v10.html')
    source=pinned_file(ROOT,'src/main/cljs/cljs/core.cljs').decode()
    assert (FOLDER/'NOTICE').read_text().rstrip('\n')==source.split('\n\n')[0]
    check();compare(FOLDER/'evidence/observations.json')
    receipt=read(FOLDER/'evidence/receipt.json')
    assert receipt['upstream']==PIN and receipt['reference']['cases']==151
    assert receipt['native']=='UNCOMPILED/UNEXECUTED; bootstrap unchanged/stale'
    for path,expected in receipt['sha256'].items():
        assert digest((ROOT/path).read_bytes())==expected,path
    print('Verified complete Error parents, EPL, 151 raw observations and authored receipt hashes; native pending')
if __name__=='__main__':verify()
