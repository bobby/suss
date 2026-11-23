from pathlib import Path
import hashlib,json,subprocess
root=Path(__file__).resolve().parents[2];out=Path(__file__).resolve().parent
paths=subprocess.check_output(['git','diff','--name-only'],cwd=root,text=True).splitlines()+['crates/suss-compile/tests/bigint_helper.rs','results/m12-helper-multiply/cases.json']
(out/'inputs.json').write_text(json.dumps({p:hashlib.sha256((root/p).read_bytes()).hexdigest() for p in paths if (root/p).is_file()},indent=2)+'\n')
