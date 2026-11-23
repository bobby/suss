"""Observe every conversion as a native CLI result; language errors stay values."""
from pathlib import Path
import hashlib,json,os,subprocess,sys,time
ROOT=Path(__file__).resolve().parents[2];OUT=Path(__file__).resolve().parent
label,fixture=sys.argv[1:];assert not (OUT/(label+'.json')).exists()
cases=json.loads((OUT/fixture).read_text())
source='['+'\n'.join('(pr-str (try '+e.replace('(catch Throwable ','(catch :default ')+' (catch :default e [:error (ex-message e)])))' for e,_ in cases)+']\n'
path=OUT/(label+'.sus');path.write_text(source);exe=ROOT.parent/'m12-decimal-target/debug/suss'
command=['systemd-run','--user','--scope','-p','MemoryMax=4G','--',str(exe),str(path)]
record={'command':command,'head':subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),'exe_sha256':hashlib.sha256(exe.read_bytes()).hexdigest(),'source_sha256':hashlib.sha256(source.encode()).hexdigest(),'fixture_sha256':hashlib.sha256((OUT/fixture).read_bytes()).hexdigest(),'harness_sha256':hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),'complete':False}
def save():(OUT/(label+'.json')).write_text(json.dumps(record,indent=2)+'\n')
save();started=time.time();p=subprocess.run(command,cwd=ROOT,env=os.environ|{'XDG_RUNTIME_DIR':'/run/user/1000'},capture_output=True,text=True)
(OUT/(label+'.stdout')).write_text(p.stdout);(OUT/(label+'.stderr')).write_text(p.stderr)
record.update(complete=True,exit_code=p.returncode,seconds=time.time()-started);save()
if p.returncode==0:
 text=p.stdout.strip();assert text[0]=='[' and text[-1]==']',text[:100];index=1;values=[];decoder=json.JSONDecoder()
 while index<len(text)-1:
  while text[index].isspace() or text[index]==',':index+=1
  if text[index]==']':break
  value,length=decoder.raw_decode(text[index:]);assert isinstance(value,str);values.append(value);index+=length
 assert len(values)==len(cases),(len(values),len(cases))
 record['mismatches']=[{'index':i,'source':expr,'expected':expected,'actual':value} for i,((expr,expected),value) in enumerate(zip(cases,values)) if value!=expected];record['matched']=len(cases)-len(record['mismatches']);save()
print({k:v for k,v in record.items() if k not in ['command','mismatches']},flush=True)
