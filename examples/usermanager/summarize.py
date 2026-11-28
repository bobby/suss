#!/usr/bin/env python3
"""Summarize raw inclusive measurements without removing startup or host costs."""
import argparse
import json
from pathlib import Path
import statistics

def spread(values):
    return {'count':len(values),'minimum':min(values),'median':statistics.median(values),'maximum':max(values)}

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('directory',type=Path);a=p.parse_args()
    groups={}
    def add(key, value):
        groups.setdefault(key,[]).append(value)
    failures=[]
    for path in sorted(a.directory.glob('*.receipt.json')):
        r=json.loads(path.read_text())
        if r['exit_code']!=0:failures.append(path.name);continue
        name=path.name.removesuffix('.receipt.json')
        group='-'.join(name.split('-')[:-1]) if name.rsplit('-',1)[-1].isdigit() else name
        add(group+'/wall_ns',r['wall_ns'])
        add(group+'/peak_rss_kib',r['rusage']['peak_rss_kib'])
        if r['process_start_through_first_response_ns'] is not None:
            add(group+'/process_start_through_first_response_ns',r['process_start_through_first_response_ns'])
    for path in sorted(a.directory.glob('*.stdout.log')):
        name=path.name.removesuffix('.stdout.log')
        if not name.startswith(('native-','jvm-')):continue
        runtime,size,_=name.split('-');base=runtime+'/'+size
        for line in path.read_text().splitlines():
            try:r=json.loads(line)
            except json.JSONDecodeError:continue
            entries=r.get('measurements',[]) if runtime=='native' else ([r] if 'operation' in r else [])
            for entry in entries:
                for field in ['ns','host_capability_ns','host_capability_calls']:
                    value=entry.get(field,entry.get(field.replace('_','-')))
                    if value is not None:add(base+'/'+entry['operation']+'/'+field,value)
            for key in ['through_first_useful_response_ns','load_through_instantiation_ns','source-load-through-first-response-ns']:
                if key in r:add(base+'/'+key,r[key])
    result={'failures':failures,'spreads':{key:spread(value) for key,value in sorted(groups.items())},
            'units':'Nanoseconds except explicitly named RSS KiB and callback counts. Every observation is inclusive; no subtraction.',
            'qualification':'Raw logs/receipts and environment.json are authoritative. Runtime/adapter differences and concurrent system load prevent universal speed claims.'}
    (a.directory/'spreads.json').write_text(json.dumps(result,indent=2)+'\n')
    for key, values in result['spreads'].items():
        print(key,json.dumps(values))
    if failures:raise SystemExit(1)

if __name__=='__main__':main()
