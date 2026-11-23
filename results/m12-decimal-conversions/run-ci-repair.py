from pathlib import Path
import hashlib,json,os,subprocess,time
ROOT=Path(__file__).resolve().parents[2]; OUT=Path(__file__).resolve().parent/'ci-repair'
OUT.mkdir(exist_ok=True); assert not (OUT/'commands.json').exists()
env=os.environ|{'XDG_RUNTIME_DIR':'/run/user/1000','CARGO_BUILD_JOBS':'2','RUSTUP_TOOLCHAIN':'stable','CARGO_TARGET_DIR':str(ROOT.parent/'m12-decimal-followup-target')}
commands=[
('identity', ['cargo','test','--locked','-p','suss-compile','--lib','artifact_identity::tests','--','--test-threads=2']),
('reloads', ['cargo','test','--locked','-p','suss-cli','--test','core_import','--test','core_sequence_interfaces','--test','core_sorted_interfaces','--','reload','--test-threads=2']),
('size', ['cargo','test','--locked','-p','suss-cli','--test','runtime_namespaces','aot_namespace_frame_size_is_bounded_per_form','--','--exact','--nocapture','--test-threads=2']),
('retained-context', ['cargo','test','--locked','-p','suss-cli','--test','clojure_decimal_conversions','reloaded_core_context_getter','--','--test-threads=2']),
('artifact-execution', ['cargo','test','--locked','-p','suss-cli','--test','compiled_artifact_identities','--','--test-threads=2']),
('size-breakdown', ['cargo','run','--profile','test','--locked','-p','suss-cli','--example','probe_decimal_aot_size','--',str(OUT/'aot-size-after')])]
paths=subprocess.check_output(['git','ls-files','--cached','--others','--exclude-standard','-z'],cwd=ROOT).decode().split('\0')
inputs={p:hashlib.sha256((ROOT/p).read_bytes()).hexdigest() for p in paths if p and not p.startswith('results/') and (ROOT/p).is_file()}
record={'head':subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),'inputs':inputs,'requested_commands':commands,'commands':[],'complete':False}
def save():(OUT/'commands.json').write_text(json.dumps(record,indent=2)+'\n')
save()
for label,command in commands:
 t=time.time()
 with (OUT/(label+'.log')).open('w') as log:r=subprocess.run(['systemd-run','--user','--scope','-p','MemoryMax=4G','--',*command],cwd=ROOT,env=env,stdout=log,stderr=subprocess.STDOUT)
 row={'label':label,'command':command,'exit_code':r.returncode,'seconds':time.time()-t};record['commands'].append(row);save();print(row,flush=True)
 if r.returncode:break
record.update(complete=True,inputs_unchanged=all(hashlib.sha256((ROOT/p).read_bytes()).hexdigest()==h for p,h in inputs.items()));save()
