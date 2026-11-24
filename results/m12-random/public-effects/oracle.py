from pathlib import Path
import hashlib,json,subprocess,time
BASE=Path(__file__).resolve().parent
p=json.loads((BASE.parent/'source-provenance.json').read_text());jars=p['jars'];sha=lambda b:hashlib.sha256(b).hexdigest()
assert all(sha(Path(path).read_bytes())==value for path,value in jars.items())
cases=[
('default-and-bound-return-types','[(double? (rand)) (double? (rand 5)) (integer? (rand-int 5))]'),
('zero-bound','[(rand 0) (rand-int 0)]'),
('negative-and-fractional-bounds','(let [a (rand -5) b (rand 5.5) c (rand-int -5) d (rand-int 5.5)] [(< -5 a) (<= a 0) (<= 0 b) (< b 5.5) (<= -4 c) (<= c 0) (<= 0 d) (<= d 5)])'),
('exact-number-bounds','(let [a (rand 5N) b (rand 1/3) c (rand 1.25M)] [(double? a) (double? b) (double? c) (<= 0 a) (< a 5) (<= 0 b) (< b 1/3) (<= 0 c) (< c 1.25M)])'),
('rand-bound-once','(let [trace (atom []) value (rand (do (swap! trace conj :bound) 0))] [value @trace])'),
('rand-int-bound-once','(let [trace (atom []) value (rand-int (do (swap! trace conj :bound) 0))] [value @trace])'),
('invalid-rand-bound-effects','(let [trace (atom []) result (try (rand (do (swap! trace conj :bound) :invalid)) (catch Throwable e :error))] [result @trace])'),
('invalid-rand-int-bound-effects','(let [trace (atom []) result (try (rand-int (do (swap! trace conj :bound) :invalid)) (catch Throwable e :error))] [result @trace])'),
('rand-invalid-arity-argument-order','(let [trace (atom []) result (try (rand (do (swap! trace conj :left) 0) (do (swap! trace conj :right) 0)) (catch Throwable e :error))] [result @trace])'),
('rand-int-zero-arity','(try (rand-int) (catch Throwable e :error))'),
('rand-int-invalid-arity-argument-order','(let [trace (atom []) result (try (rand-int (do (swap! trace conj :left) 0) (do (swap! trace conj :right) 0)) (catch Throwable e :error))] [result @trace])'),
('first-class-functions','(let [f rand g rand-int] [(double? (f)) (f 0) (g 0)])'),
('apply-functions','[(double? (apply rand [])) (apply rand [0]) (apply rand-int [0])]'),
('normal-stream-has-distinct-values','(let [xs ['+' '.join(['(rand)']*16)+']] [(every? #(and (double? %) (<= 0 %) (< % 1)) xs) (> (count (set xs)) 1)])'),
('negative-zero-bound','[(pr-str (rand -0.0)) (rand-int -0.0)]'),
('recovery-after-error','[(try (rand :invalid) (catch Throwable e :error)) (rand 0) (rand-int 0)]'),
]
source='(assert (= "1.12.6" (clojure-version)))\n'+''.join('(println (pr-str (pr-str '+expr+')))\n' for _,expr in cases)
(BASE/'oracle.clj').write_text(source)
command=['java','-Xmx512m','-cp',':'.join(jars),'clojure.main',str(BASE/'oracle.clj')]
start=time.time();result=subprocess.run(command,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
(BASE/'stdout.log').write_bytes(result.stdout);(BASE/'stderr.log').write_bytes(result.stderr)
record={'command':command,'exit_code':result.returncode,'seconds':time.time()-start,'jars':jars,'source_sha256':sha(source.encode()),'stdout_sha256':sha(result.stdout),'controls':len(cases),'qualification':'Actual public API calls. Expected values contain types, zero results, argument-effect traces, ranges and stream properties; no nondeterministic draw is used as an exact native golden. The sixteen-draw distinctness assertion has a negligible nonzero random collision probability.'}
(BASE/'command.json').write_text(json.dumps(record,indent=2)+'\n')
assert result.returncode==0,result.stderr.decode()
values=[json.loads(line) for line in result.stdout.decode().splitlines()];assert len(values)==len(cases)
(BASE/'cases.json').write_text(json.dumps([dict(name=name,expression=expr,expected=value) for (name,expr),value in zip(cases,values)],indent=2)+'\n')
print({k:v for k,v in record.items() if k not in ['command','jars']})
