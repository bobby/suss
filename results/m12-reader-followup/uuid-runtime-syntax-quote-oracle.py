"""Fresh primary structures for runtime syntax quote of atomic UUID data."""
from pathlib import Path
import hashlib,json,subprocess,time
ROOT=Path(__file__).resolve().parents[2];BASE=Path(__file__).resolve().parent
OUT=BASE/'uuid-runtime-syntax-quote-primary';OUT.mkdir(exist_ok=True)
assert not (OUT/'command.json').exists()
source=r'''(doseq [[label input] [["short" "`#uuid \"1-2-3-4-5\""]
                              ["signed" "`#uuid \"80000000-0000-0001-ffff-fffffffffffe\""]
                              ["positive" "`#uuid \"7fffffff-ffff-fffe-8000-000000000001\""]]]
  (let [source (str "(read-string " (pr-str input) ")")
        form (read-string input)
        value (second form)]
    (println (str "[" (pr-str label) "," (pr-str source) ","
                  (pr-str (pr-str form)) "," (pr-str (str (first form))) ","
                  (pr-str (str (.getMostSignificantBits ^java.util.UUID value))) ","
                  (pr-str (str (.getLeastSignificantBits ^java.util.UUID value))) "]"))))
'''
program=OUT/'oracle.clj';program.write_text(source)
jars=json.loads((BASE/'reader-oracle-command.json').read_text())['jars']
command=['java','-Xmx128m','-XX:ActiveProcessorCount=1','-cp',':'.join(jars),'clojure.main',str(program)]
started=time.time();result=subprocess.run(command,cwd=ROOT,capture_output=True)
(OUT/'stdout').write_bytes(result.stdout);(OUT/'stderr').write_bytes(result.stderr)
record={'command':command,'exit_code':result.returncode,'seconds':time.time()-started,
 'source_sha256':hashlib.sha256(program.read_bytes()).hexdigest(),
 'jars':{p:hashlib.sha256(Path(p).read_bytes()).hexdigest() for p in jars},
 'stdout_sha256':hashlib.sha256(result.stdout).hexdigest()}
(OUT/'command.json').write_text(json.dumps(record,indent=2)+'\n')
assert result.returncode==0,result.stderr.decode()
rows=[dict(zip(['name','source','printed','head','most','least'],json.loads(line))) for line in result.stdout.decode().splitlines()]
assert len(rows)==3 and all(row['head']=='quote' for row in rows)
(OUT/'cases.json').write_text(json.dumps(rows,indent=2)+'\n');print(rows)
