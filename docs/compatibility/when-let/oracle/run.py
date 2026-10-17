import hashlib,json,os,pathlib,subprocess,time
p=pathlib.Path(__file__).parent
repo=pathlib.Path('/home/bobby/Work/code/bobby/suss-m4-when-let')
pin=repo/'clojurescript'
assert subprocess.check_output(['git','-C',str(pin),'rev-parse','HEAD'],text=True).strip()=='c4295f303100bbf5afac449242d30bca1126f1a1'
assert not subprocess.check_output(['git','-C',str(pin),'status','--porcelain'])
env=os.environ.copy()
env['JAVA_HOME']='/home/bobby/.cache/suss-m4-oracle-tools/jdk-21.0.8+9-jre'
env['PATH']=env['JAVA_HOME']+'/bin:'+env['PATH']
env['CLJ_CONFIG']='/tmp/suss-oracle-clojure-config'
env['CLJ_CACHE']=str(p/'clj-cache')
source=p/'src/suss_oracle/when_let_probe.cljs'
for attempt in ['run1','run2']:
    opts='{:force true :cache-analysis false :target :nodejs :output-to "out/'+attempt+'/probe.js" :output-dir "out/'+attempt+'/cljs" :optimizations :none :source-map false}'
    commands=[['/home/bobby/.cache/suss-m4-oracle-tools/bin/clojure','-J-Xmx1g','-J-XX:ActiveProcessorCount=2','-Srepro','-M','-m','cljs.main','-co',opts,'-c','suss-oracle.when-let-probe'],['node','out/'+attempt+'/probe.js']]
    receipt={'commands':commands,'source_sha256':hashlib.sha256(source.read_bytes()).hexdigest(),'steps':[],'scope':'Pinned development CLJS/Node only; no Suss execution.'}
    for index,command in enumerate(commands):
        start=time.monotonic()
        log=p/(attempt+'-'+str(index)+'.log')
        with log.open('wb') as out:
            code=subprocess.call(command,cwd=p,env=env,stdout=out,stderr=subprocess.STDOUT)
        receipt['steps'].append({'exit':code,'seconds':time.monotonic()-start,'log':log.name})
        receipt['source_unchanged']=hashlib.sha256(source.read_bytes()).hexdigest()==receipt['source_sha256']
        (p/(attempt+'-receipt.json')).write_text(json.dumps(receipt,indent=2)+'\n')
        print(attempt,index,code,flush=True)
        if code: raise SystemExit(code)
    value=json.loads((p/(attempt+'-1.log')).read_text())
    (p/(attempt+'-observations.json')).write_text(json.dumps(value,indent=2)+'\n')
assert (p/'run1-observations.json').read_bytes()==(p/'run2-observations.json').read_bytes()
print('two fresh compile/Node runs match',flush=True)
