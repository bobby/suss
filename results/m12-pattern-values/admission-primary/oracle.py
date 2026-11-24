"""Actual pinned JVM regex compilation/printing evidence; no Suss pass is implied."""
from pathlib import Path
import hashlib,json,subprocess,time
BASE=Path(__file__).resolve().parent;old=json.loads((BASE.parent/'m12-reader-followup/reader-oracle-command.json').read_text())
patterns=['','my regex','a"b',r'\d+',r'\Q[a-z]+\E',r'\Qabc',r'\E',r'(?i)a',r'(?im)a',r'(?s:.)',r'(?-i:a)',r'(?U)\w',r'(?u)\w',r'(?d).',r'(?x)a #comment\nb',r'(?c)a',r'(?#comment)a',r'(?q)a',r'(?R)',r'(?|a|b)',r'(?(1)a|b)',r'[a-z&&[^bc]]',r'[a-z&&b-d]',r'[[a-z][0-9]]',r'[]a]',r'[^]a]',r'[]',r'[^]',r'[z-a]',r'[a-]',r'[-a]',r'[^a]',r'(?<=a)b',r'(?<!a)b',r'(?=a)a',r'(?!a)b',r'(?>a|ab)b',r'(a)\1',r'\1',r'(?<word>a)\k<word>',r'(?<word>a)\k<other>',r'(?<1>a)',r'(?<a>a)(?<a>b)',r'a*+',r'a++',r'a?+',r'a{2,3}+',r'a{3,2}',r'a{',r'a{,3}',r'a{2}',r'a{2,}',r'*',r'+',r'?',r'(',r')',r'[',chr(92),r'\y',r'\a\e\cA',r'\R\h\H\v\V\X',r'\p{Lu}',r'\p{IsLatin}',r'\p{javaLowerCase}',r'\p{Unknown}',r'\N{LATIN CAPITAL LETTER A}',r'\x{1F600}',r'\uD83D\uDE00',r'\u123',r'\xGG',r'\0123','\ud800','\udfff','😀','a\x00b']
patterns=list(dict.fromkeys(patterns));units=[[int.from_bytes(raw[i:i+2],'little') for i in range(0,len(raw),2)] for raw in [s.encode('utf-16-le','surrogatepass') for s in patterns]]
input=BASE/'pattern-inputs.edn';input.write_text(str(units).replace(',',' ')+'\n')
source=BASE/'pattern-oracle.clj';source.write_text('''(assert (= "1.12.6" (clojure-version)))
(let [rows (mapv (fn [units]
  (let [text (String. (char-array (map char units)))]
    (try (let [p (re-pattern text)]
      {:units units :valid true :pattern-units (mapv int (.pattern p)) :flags (.flags p)
       :str-units (mapv int (str p)) :printed-units (mapv int (pr-str p))})
      (catch Throwable e
        {:units units :valid false :exception (.getName (class e))
         :message-units (mapv int (ex-message e))
         :description-units (when (instance? java.util.regex.PatternSyntaxException e)
                              (mapv int (.getDescription e)))
         :index (when (instance? java.util.regex.PatternSyntaxException e) (.getIndex e))}))))
  (read-string (slurp (first *command-line-args*))))]
  (spit (second *command-line-args*) (pr-str rows))
  (println (pr-str {:cases (count rows) :accepted (count (filter :valid rows))
                   :rejected (count (remove :valid rows))})))
''')
cmd=old['command'][:-1]+[str(source),str(input),str(BASE/'pattern-cases.edn')];start=time.time();r=subprocess.run(cmd,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True);(BASE/'oracle.log').write_text(r.stdout)
record={'command':cmd,'exit_code':r.returncode,'seconds':time.time()-start,'jars':old['jars'],'source_sha256':hashlib.sha256(source.read_bytes()).hexdigest(),'input_sha256':hashlib.sha256(input.read_bytes()).hexdigest(),'java_version':subprocess.check_output(['java','-version'],stderr=subprocess.STDOUT,text=True)}
if r.returncode==0:record['cases']=len(patterns);record['output_sha256']=hashlib.sha256((BASE/'pattern-cases.edn').read_bytes()).hexdigest()
(BASE/'command.json').write_text(json.dumps(record,indent=2)+'\n');print(r.stdout[-1500:]);raise SystemExit(r.returncode)
