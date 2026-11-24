"""Fresh public eval observations; these are not native Suss pass claims."""
from pathlib import Path
import hashlib,json,subprocess,time,zipfile
BASE=Path(__file__).resolve().parent
jars=json.loads((BASE.parents[1]/'m12-random/source-provenance.json').read_text())['jars']
sha=lambda b:hashlib.sha256(b).hexdigest()
assert all(sha(Path(path).read_bytes())==value for path,value in jars.items())
jar=next(Path(path) for path in jars if path.endswith('/clojure-1.12.6.jar'))
with zipfile.ZipFile(jar) as z:core=z.read('clojure/core.clj')
start=core.index(b'(defn eval\n');end=core.index(b'\n\n',start)
original=core[start:end]+b'\n';(BASE/'upstream-eval.clj').write_bytes(original)
cases=[
('numeric-literals','[(eval 1N) (eval 1/2) (eval 1.50M)]'),
('quoted-numeric-application',"(eval '(+ 1N 1/2))"),
('self-evaluating-values','[(eval 1) (eval 0) (eval -1) (eval 1.0) (eval "a string") (eval "(+ 1 2)") (eval \\x) (eval true) (eval false) (eval nil) (eval :a-keyword)]'),
('empty-list',"(eval '())"),
('embedded-global-symbol',"(eval [:a :b 'user/eval-fixture-value])"),
('collection-values',"[(eval [:a :b]) (eval {:a :b}) (eval #{:a :b})]"),
('compiled-function',"(let [f (eval '(fn [x] (+ x 1N)))] [(fn? f) (f 2)])"),
('existing-function-identity',"(let [f (fn [x] x)] (identical? (eval f) f))"),
('function-symbol',"[(fn? (eval '+)) (fn? (eval +))]"),
('qualified-global',"(eval 'user/eval-fixture-value)"),
('namespace-selection',"(binding [*ns* (the-ns 'eval-fixture)] (eval 'value))"),
('lexical-locals-are-not-captured',"(let [only-local 7] (try (eval 'only-local) (catch Throwable e :unresolved)))"),
('ordinary-var-result',"(let [v (eval '(def eval-created 17))] [(var? v) (deref v) (:name (meta v))])"),
('stable-var-identity',"(identical? (var eval-created) (eval '(var eval-created)))"),
('macro-and-loop',"(eval '(let [initial 0] (loop [x initial] (if (= x 43) x (recur (inc x))))))"),
('user-compiled-macro',"(eval '(eval-fixture-macro))"),
('recursive-eval',"[(eval '(eval 1N)) (eval '(eval (eval 1/2)))]"),
('lazy-result-not-realized',"(first (eval '(range)))"),
('dynamic-math-context',"(with-precision 5 (eval '(/ 1M 3M)))"),
('wrong-arity-evaluates-arguments',"(let [trace (atom []) result (try (eval (do (swap! trace conj :left) 1) (do (swap! trace conj :right) 2)) (catch Throwable e :arity))] [result @trace])"),
('do-prefix-effects-before-compile-error',"(let [trace (atom []) result (try (eval '(do (swap! user/eval-fixture-effects conj :prefix) eval-missing-name)) (catch Throwable e :compile))] [result @user/eval-fixture-effects])"),
('failed-initializer-preserves-root',"(let [result (try (eval '(def eval-existing (throw (ex-info \"failure\" {})))) (catch Throwable e :throw))] [result eval-existing])"),
('generated-data-is-compiled',"(eval (list '+ 1N 1/2))"),
('fresh-global-definition',"(do (eval '(def eval-fresh 21)) (eval '(* user/eval-fresh 2)))"),
]
prelude='''(assert (= "1.12.6" (clojure-version)))
(def eval-fixture-value 42)
(def eval-fixture-effects (atom []))
(def eval-existing 99)
(create-ns 'eval-fixture)
(intern 'eval-fixture 'value 73)
(defmacro eval-fixture-macro [] (list 'quote :expanded))
'''
source=prelude+''.join('(println (pr-str (pr-str '+expr+')))\n' for _,expr in cases)
(BASE/'oracle.clj').write_text(source)
command=['java','-Xmx512m','-cp',':'.join(jars),'clojure.main',str(BASE/'oracle.clj')]
start=time.time();r=subprocess.run(command,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
(BASE/'stdout.log').write_bytes(r.stdout);(BASE/'stderr.log').write_bytes(r.stderr)
record={'command':command,'exit_code':r.returncode,'seconds':time.time()-start,'jars':jars,'entry':'clojure/core.clj','entry_sha256':sha(core),'upstream_eval_sha256':sha(original),'source_sha256':sha(source.encode()),'stdout_sha256':sha(r.stdout),'controls':len(cases),'qualification':'Primary semantic observations for a missing real compiled eval dependency. No native implementation, public eval route, Var support, affected-body or numeric acceptance pass is established.'}
(BASE/'command.json').write_text(json.dumps(record,indent=2)+'\n')
assert r.returncode==0,r.stderr.decode()
values=[json.loads(line) for line in r.stdout.decode().splitlines()];assert len(values)==len(cases)
(BASE/'cases.json').write_text(json.dumps([dict(name=name,expression=expr,expected=value) for (name,expr),value in zip(cases,values)],indent=2)+'\n')
print({k:v for k,v in record.items() if k not in ['command','jars']})
