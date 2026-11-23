"""Lossless UUID parser data from actual pinned JVM, including UTF-16 errors."""
from pathlib import Path
import hashlib,json,random,subprocess,time
OUT=Path(__file__).resolve().parent;old=json.loads((OUT/'reader-oracle-command.json').read_text());inputs=json.loads((OUT/'reader-inputs.json').read_text())
# Preserve original fixture classes; classification comes from the executed JVM.
texts=['550e8400-e29b-41d4-a716-446655440000','00000000-0000-0000-0000-000000000000','ffffffff-ffff-ffff-ffff-ffffffffffff','1-2-3-4-5','+1-+2-+3-+4-+5','\uff11-\uff12-\uff13-\uff14-\uff15','\u0661-\u0662-\u0663-\u0664-\u0665','','not-a-uuid','1-2-3-4','-1-2-3-4','1--2-3-4','1-2-3-4-','1-2-3-4-g','0x1-2-3-4-5','8000000000000000-2-3-4-5','0000000000000000000000000001-2-3-4-5','1-2-3-4-+','1-2-3-4-\ud800']
for position in range(5):
 for field in ['0','+0','+','ffffffff','100000000','ffffffffffff','1000000000000','7fffffffffffffff','8000000000000000','00000000000000000000000001','\uff21\uff22\uff23','\uff41\uff42\uff43']:
  parts=['0']*5;parts[position]=field;texts.append('-'.join(parts))
rng=random.Random(506)
for i in range(160):
 parts=[]
 for width in [8,4,4,4,12]:
  value=rng.getrandbits(width*4);part=f'{value:x}'
  if i%7==0:part='+'+part
  if i%11==0:part='00'+part
  if i%13==0:part=part.translate(str.maketrans('0123456789abcdef','\uff10\uff11\uff12\uff13\uff14\uff15\uff16\uff17\uff18\uff19\uff41\uff42\uff43\uff44\uff45\uff46'))
  parts.append(part)
 texts.append('-'.join(parts))
texts=list(dict.fromkeys(texts));units=[list(int.from_bytes(s.encode('utf-16-le','surrogatepass')[i:i+2],'little') for i in range(0,len(s.encode('utf-16-le','surrogatepass')),2)) for s in texts]
(OUT/'uuid-parser-inputs.edn').write_text(str(units).replace(',', ' ')+'\n')
source=OUT/'uuid-parser-oracle.clj';source.write_text('''(assert (= "1.12.6" (clojure-version)))
(let [rows (mapv (fn [units]
  (let [text (String. (char-array (map char units)))]
    (try (let [u (java.util.UUID/fromString text)]
      {:units units :bits [(.getMostSignificantBits u) (.getLeastSignificantBits u)]
       :canonical (str u) :hash (.hashCode u) :error-units nil})
      (catch Throwable e {:units units :bits nil :canonical nil :hash nil
                          :error-units (mapv int (ex-message e))}))))
(read-string (slurp (nth *command-line-args* 2))))]
  (spit (first *command-line-args*) (pr-str rows))
  (println (pr-str {:cases (count rows) :accepted (count (filter :bits rows)) :rejected (count (remove :bits rows))})))
(with-open [out (clojure.java.io/output-stream (second *command-line-args*))]
  (.write out (byte-array (map #(inc (Character/digit (char %) 16)) (range 65536)))))
''')
command=old['command'][:-1]+[str(source.resolve()),str((OUT/'uuid-parser-cases.edn').resolve()),str((OUT/'uuid-hex-digit-jvm.bin').resolve()),str((OUT/'uuid-parser-inputs.edn').resolve())];start=time.time();r=subprocess.run(command,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True);(OUT/'uuid-parser-oracle.log').write_text(r.stdout)
record={'command':command,'exit_code':r.returncode,'seconds':time.time()-start,'source_sha256':hashlib.sha256(source.read_bytes()).hexdigest(),'jars':old['jars']}
if r.returncode==0:record['outputs']={p.name:hashlib.sha256(p.read_bytes()).hexdigest() for p in [OUT/'uuid-parser-cases.edn',OUT/'uuid-hex-digit-jvm.bin']}
(OUT/'uuid-parser-oracle-command.json').write_text(json.dumps(record,indent=2)+'\n');print(r.stdout[-1500:]);raise SystemExit(r.returncode)
