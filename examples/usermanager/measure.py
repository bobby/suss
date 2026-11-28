#!/usr/bin/env python3
"""Measure the actual CLI compiler, component/native host and shared JVM app."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import subprocess
import time

ROOT = Path(__file__).resolve().parents[2]  # examples/usermanager/measure.py

def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def write(path, value):
    path.write_text(json.dumps(value, indent=2) + '\n')

def run(output, name, command, cwd=ROOT, stdin=None):
    before = time.monotonic_ns()
    load = os.getloadavg()
    first = None
    lines = []
    with (output / (name + '.stderr.log')).open('w') as errors:
        process = subprocess.Popen(command, cwd=cwd, stdin=subprocess.PIPE if stdin else subprocess.DEVNULL,
                                   stdout=subprocess.PIPE, stderr=errors, text=True)
        if stdin:
            process.stdin.write(stdin)
            process.stdin.close()
        for line in process.stdout:
            lines.append(line)
            if '"phase":"first-response"' in line.replace(' ', '') and first is None:
                first = time.monotonic_ns() - before
        _, status, usage = os.wait4(process.pid, 0)
        process.returncode = os.waitstatus_to_exitcode(status)
    (output / (name + '.stdout.log')).write_text(''.join(lines))
    result = {'command': command, 'cwd': str(cwd), 'exit_code': process.returncode,
              'wall_ns': time.monotonic_ns()-before, 'process_start_through_first_response_ns': first,
              'load_before': load, 'load_after': os.getloadavg(),
              'rusage': {'peak_rss_kib': usage.ru_maxrss, 'user_seconds': usage.ru_utime,
                         'system_seconds': usage.ru_stime, 'minor_faults': usage.ru_minflt,
                         'major_faults': usage.ru_majflt, 'voluntary_context_switches': usage.ru_nvcsw,
                         'involuntary_context_switches': usage.ru_nivcsw}}
    write(output / (name + '.receipt.json'), result)
    print(name, process.returncode, result['wall_ns'], flush=True)
    if process.returncode:
        raise SystemExit(process.returncode)
    return lines

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for flag in ('suss', 'host', 'measure', 'output'):
        parser.add_argument('--' + flag, type=Path, required=True)
    parser.add_argument('--repetitions', type=int, default=3)
    parser.add_argument('--sizes', type=int, nargs='+', default=[16, 1000, 10000])
    args = parser.parse_args()
    if args.repetitions < 1 or min(args.sizes) < 1:
        parser.error('positive sizes and repetitions required')
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    suss, host, measure = [str(p.resolve()) for p in (args.suss, args.host, args.measure)]
    sources = subprocess.check_output(['git','ls-files','-co','--exclude-standard'],cwd=ROOT,text=True).splitlines()
    identities = {name:sha(ROOT/name) for name in sorted(set(sources))
                  if (ROOT/name).is_file() and not name.startswith('results/') and output not in (ROOT/name).parents}
    write(output/'environment.json', {'machine':platform.uname()._asdict(),
          'head':subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),
          'clojurescript_pin':subprocess.check_output(['git','-C','clojurescript','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),
          'cgroup_limits':{name:(Path('/sys/fs/cgroup')/Path('/proc/self/cgroup').read_text().strip().split('::',1)[1].lstrip('/')/name).read_text() for name in ['cpu.max','memory.high','memory.max']},
          'cpuinfo':Path('/proc/cpuinfo').read_text(), 'meminfo':Path('/proc/meminfo').read_text(),
          'cpu_affinity':sorted(os.sched_getaffinity(0)), 'cgroup':Path('/proc/self/cgroup').read_text(),
          'processes':subprocess.check_output(['ps','-eo','pid,comm,args'],text=True),
          'native_binaries':{p:{'bytes':Path(p).stat().st_size,'sha256':sha(Path(p))} for p in (suss,host,measure)},
          'versions':{tool:subprocess.check_output([tool,'--version'],stderr=subprocess.STDOUT,text=True)
                      for tool in ('cargo','rustc','java')}, 'inputs':identities,
          'qualifications':['Native --profile test (workspace opt-level1, dependencies opt-level2), debug assertions on; Wasmtime OptLevel::None.',
             'Fresh compiler cache means an initially empty explicit build directory; OS/filesystem caches are not dropped.',
             'Concurrent baseline processes and cgroup CPU/memory limits are recorded; these are machine-specific baselines.',
             'JVM shared app uses persistent in-memory Clojure maps; Rust uses BTreeMaps/JSON persistence, serializing the whole database before each write for rollback.',
             'Callback timing includes instrumentation; it is reported alongside inclusive end-to-end time without subtraction.',
             'Exact canonical allocation baseline is independently observed by the checked-in lifetime tests; unmodified application measurement exports no allocator counter.']})
    components=[]
    for repetition in range(args.repetitions):
        component=output/f'application-{repetition}.wasm'
        command=['python3','examples/usermanager/build.py','--suss',suss,'--output',str(component),
                 '--build-dir',str(output/f'compiler-cache-{repetition}')]
        run(output,f'compile-fresh-{repetition}',command)
        cold_hash=sha(component)
        run(output,f'compile-cached-{repetition}',command)
        assert sha(component)==cold_hash, 'cached compile changed component bytes'
        components.append({'path':str(component),'bytes':component.stat().st_size,'sha256':cold_hash})
    write(output/'components.json',components)
    component=components[0]['path']
    # Usable transport + actual persistent storage, including failures and reset.
    requests=[{'op':'seed-department','id':4,'name':'Development'},
              {'op':'save-user','form':{'first_name':'λ🦀','last_name':'Example','department_id':'４'}},
              {'op':'list-users'}, {'op':'edit-user','id':1},
              {'op':'save-user','form':{'id':'1','email':None,'department_id':'4'}},
              {'op':'save-user','form':{'id':'bad','department_id':'4'}},
              {'op':'reset-changes'}]
    transport=''.join(json.dumps(r,ensure_ascii=False)+'\n' for r in requests)
    persisted=output/'storage.json'
    first=run(output,'interactive-write',[host,component,str(persisted)],stdin=transport)
    assert len(first)==len(requests)
    assert 'Err' in json.loads(first[5]), 'malformed ID must fail'
    second=run(output,'interactive-reopen',[host,component,str(persisted)],stdin='{"op":"list-users"}\n')
    assert len(json.loads(second[0])['Ok']['users'])==1, 'reopened row missing'
    for name in ['interactive-write','interactive-reopen']:
        counters=json.loads((output/(name+'.stderr.log')).read_text().splitlines()[-1])
        assert counters['opened']==counters['closed']==1 and counters['live']==0, counters
    for repetition in range(args.repetitions):
        for size in args.sizes:
            run(output,f'native-{size}-{repetition}',[measure,component,str(size)])
            run(output,f'jvm-{size}-{repetition}', ['clojure','-J-Xms64m','-J-Xmx1536m','-M',
                str(ROOT/'examples/usermanager/oracle/measure.clj'),str(size)],cwd=ROOT/'examples/usermanager')
    changed=[n for n,s in identities.items() if not (ROOT/n).is_file() or sha(ROOT/n)!=s]
    write(output/'source-audit.json', {'changed_inputs':changed,
          'native_binaries_after':{p:sha(Path(p)) for p in (suss,host,measure)}})
    assert not changed, 'measurement source changed'
    initial=json.loads((output/'environment.json').read_text())['native_binaries']
    assert all(sha(Path(p))==initial[p]['sha256'] for p in (suss,host,measure)), 'native executable changed'
    subprocess.run(['python3','examples/usermanager/summarize.py',str(output)],cwd=ROOT,check=True)

if __name__ == '__main__':
    main()
