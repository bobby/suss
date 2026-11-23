from pathlib import Path
import json,subprocess,hashlib,time,random
out=Path(__file__).resolve().parent;root=out.parents[1]
forms=[
 '(= 1 1N)', '(= 1N 1N)', '(= 0N 0)', '(= 1N 1.0)', '(= 9223372036854775808N 9223372036854775809N)',
 '[(< 9223372036854775808N 9223372036854775809N) (> -9223372036854775808N -9223372036854775809N) (<= 1N 1) (>= 1 1N)]',
 '[(== 9223372036854775808N 9223372036854775809N) (== 1N 1.0) (compare 9223372036854775808N 9223372036854775809N)]',
 '[(hash 0N) (hash 1N) (hash -1N) (hash 9223372036854775807N) (hash -9223372036854775808N) (hash 9223372036854775808N) (hash -9223372036854775809N)]',
 '[(get {1N :one} 1) (get {1 :one} 1N) (contains? #{9223372036854775808N} 9223372036854775808N) (= #{1N 2N} #{1 2})]',
 '[(integer? 1N) (integer? -9223372036854775809N) (integer? 1.0) (even? 18446744073709551616N) (odd? 18446744073709551617N)]',
 '[(long 9007199254740993N) (unchecked-long 18446744073709551615N) (unchecked-int 4294967297N) (unchecked-short 65537N) (unchecked-byte 257N)]',
 '(long 9223372036854775808N)', '(int 2147483648N)', '(byte 128N)', '(short -32769N)',
 '[(int 18446744073709551617N) (short 18446744073709551617N) (byte 18446744073709551617N)]',
 "[(+' 9223372036854775807 1) (-' -9223372036854775808 1) (*' 9223372036854775807 2) (inc' 9223372036854775807) (dec' -9223372036854775808)]",
 "[(+' 1 2) (-' 1 2) (*' 3 4) (-' -9223372036854775808) (-' 0.0) (-' -0.0) (+' 1N 2)]",
 "[(+' 1 2 3) (-' 1 2 3) (*' 2 3 4) (+' 1N 2.0) (*' 2.0 3N)]",
 "[(+' nil) (*' nil)]", "(+' nil 1)", "(*' 1 :x)",
 "(let [trace (atom []) f (fn [x] (swap! trace conj x) x)] [(+' (f 9223372036854775807) (f 1) (f 2)) @trace])",
 '[(read-string "0N") (read-string "-0N") (read-string "9223372036854775808") (read-string "-9223372036854775809")]',
 '[(read-string "0x10000000000000000N") (read-string "02000000000000000000000N") (read-string "36rZZZZZZZZZZZZZZN")]',
 '[(read-string "[1N 18446744073709551616]") (read-string "#{1N 2N}") (read-string "{1N :one}")]',
 '(read-string {:read-cond :allow :features #{:other}} "#?(:other 2 :suss 9223372036854775808N)")',
 '(read-string "08N")','(read-string "1r0N")','(read-string "2r2")',
 '[(bigint 1) (bigint 1N) (biginteger 9223372036854775808N)]',
 '[(max 9223372036854775808N 9223372036854775809N) (min -9223372036854775808N -9223372036854775809N)]',
]
forms += [
 '[(bigint 1.25) (bigint -0.999) (bigint 1.0e20) (bigint 1.0000000000000002e20) (biginteger 1.0000000000000002e20)]',
 '[(bigint "+00123") (bigint "-00123") (biginteger "18446744073709551616")]',
 '(bigint "0x10")', '(bigint "1.5")', '(bigint ##NaN)', '(bigint ##Inf)',
 '(let [x (biginteger 9223372036854775808N)] [(= x 9223372036854775808N) (= (hash x) (hash 9223372036854775808N)) (bigint x)])',
]
forms += [
 '[(quot 36893488147419103232N 8) (rem 36893488147419103233N 8) (mod -36893488147419103233N 8)]',
 '[(quot -18446744073709551617N 3N) (rem -18446744073709551617N 3N) (quot 1N 2N) (rem 1N 2N) (quot 0N 1N)]',
 '(quot 1N 0N)', '(rem 1N 0N)',
 '[(read-string "36893488147419103232/8") (read-string "-9223372036854775808/2") (read-string "-9223372036854775808/1")]',
 '(let [x (biginteger 9223372036854775808N)] (identical? x (biginteger x)))',
]
forms += [
 '(let [m (into {1N :one} (map (fn [x] [x x]) (range 2 40)))] [(get m 1) (get m 1N) (contains? m 1) (count m)])',
 '(let [m (into {1N :old} (map (fn [x] [x :new]) (range 1 40)))] [(count m) (get m 1) (get m 1N)])',
 '(let [s (into #{1N} (range 2 40))] [(contains? s 1) (contains? s 1N) (count s)])',
 '(let [m (into {1N :one} (map (fn [x] [x x]) (range 2 40)))] (with-redefs [= (fn [x y] false) number? (fn [x] false)] [(get {1N :one} 1) (get m 1) (get m 1N)]))',
]
forms += [
 "[(with-redefs [+' (fn [& xs] :rebound)] (+' 1 2)) (with-redefs [+' (fn [& xs] :rebound)] (+' 1 2 3)) (with-redefs [+' (fn [& xs] :rebound)] (+' 1))]",
 "[(with-redefs [-' (fn [& xs] :rebound)] (-' -9223372036854775808)) (with-redefs [inc' (fn [x] :rebound)] (inc' 9223372036854775807)) (with-redefs [dec' (fn [x] :rebound)] (dec' -9223372036854775808))]",
 "(let [t (atom [])] [(try (+' :bad (do (swap! t conj 1) 1) (do (swap! t conj 2) 2)) (catch :default e :error)) @t])",
 "(let [f +' g (fn [& xs] :local)] [(with-redefs [+' g] (f 1 2)) (let [+' g] (+' 1 2))])",
]
division_rng=random.Random(3380601)
for na,nb in [(1,1),(2,1),(2,2),(3,2),(8,3),(32,16),(128,64),(1,8),(8,1)]:
 a=division_rng.getrandbits(32*na);b=division_rng.getrandbits(32*nb) or 1
 if na%2:a=-a
 if nb%2:b=-b
 forms.append(f'[(quot {a}N {b}N) (rem {a}N {b}N) (mod {a}N {b}N)]')
rng=random.Random(33806)
for width in [3,4,8,32,128]:
 a=rng.getrandbits(width*32); b=a+1
 forms.append(f'[(= {a}N {a}N) (< {a}N {b}N) (< -{b}N -{a}N) (hash {a}N) (hash -{a}N)]')
observed=[f'(pr-str (try {form} (catch :default e :error)))' for form in forms]
# JVM uses Throwable; Suss uses its language catch adapter.
source='(assert (= "1.12.6" (clojure-version)))\n'+'\n'.join(f'(println (pr-str {form.replace("(catch :default e", "(catch Throwable e")}))' for form in observed)+'\n'
(out/'oracle.clj').write_text(source)
old=json.loads((root/'results/m12-bigint-values/oracle-command.json').read_text())
cmd=old['command'][:-1]+[str(out/'oracle.clj')];start=time.time();r=subprocess.run(cmd,cwd=root,capture_output=True,text=True);(out/'oracle.log').write_text(r.stdout+r.stderr)
receipt={'command':cmd,'exit_code':r.returncode,'seconds':time.time()-start,'source_sha256':hashlib.sha256(source.encode()).hexdigest(),'jars':{f:hashlib.sha256(Path(f).read_bytes()).hexdigest() for f in old['jars']}}
(out/'oracle-command.json').write_text(json.dumps(receipt,indent=2)+'\n');assert r.returncode==0,r.stderr
expected=[json.loads(line) for line in r.stdout.splitlines()];assert len(expected)==len(observed)
(out/'cases.json').write_text(json.dumps(list(zip(observed,expected)),indent=2)+'\n')
print(len(expected),'fresh primary observations')
