"""Verify complete Clojure assert, its exact portable constructor adaptation and primary data."""
from pathlib import Path
import difflib,hashlib,json,sys,zipfile
ROOT=Path(__file__).resolve().parents[2];BASE=Path(__file__).resolve().parent
sha=lambda raw:hashlib.sha256(raw).hexdigest()
recorded=json.loads((BASE/"command.json").read_text())
assert recorded["exit_code"]==0
assert all(sha(Path(p).read_bytes())==h for p,h in recorded["jars"].items())
jar=next(Path(p) for p in recorded["jars"] if p.endswith("/clojure-1.12.6.jar"))
with zipfile.ZipFile(jar) as z:source=z.read(recorded["entry"])
assert sha(source)==recorded["entry_sha256"]
original=(BASE/"upstream-assert.clj").read_bytes()
assert sha(original)==recorded["original_sha256"] and original.rstrip() in source
adapted=(BASE/"assert-adapted.sus").read_bytes()
expected=b"\n;; Copyright (c) Rich Hickey. All rights reserved. EPL-1.0.\n;; See runtime/core-import/epl-v10.html.\n"+original.replace(b"(new AssertionError ",b"(suss.bootstrap/error ")
assert original.count(b"(new AssertionError ")==2 and adapted==expected
patch="".join(difflib.unified_diff(original.decode().splitlines(True),adapted.decode().splitlines(True),fromfile="clojure.core/assert",tofile="suss.core/assert")).encode()
assert patch==(BASE/"assert.patch").read_bytes()
rust=(ROOT/"crates/suss-compile/src/portable_core_macros.rs").read_text()
marker='pub(crate) const ASSERT: &str = r#"'
assert rust.count(marker)==1 and rust.split(marker,1)[1].split('"#;',1)[0].encode()==adapted
assert (ROOT/"crates/suss-compile/src/portable_macros.rs").read_text().count("macros.define(crate::portable_core_macros::ASSERT)?;")==1
assert (ROOT/"docs/compatibility/bootstrap/exact-integers.sus").read_text().count("(def ^:dynamic *assert* true)")==1
assert sha((BASE/"oracle.clj").read_bytes())==recorded["source_sha256"]
assert sha((BASE/"stdout.log").read_bytes())==recorded["stdout_sha256"]
cases=json.loads((BASE/"cases.json").read_text());assert len(cases)==19
assert [r["expected"] for r in cases]==[json.loads(line) for line in (BASE/"stdout.log").read_text().splitlines()]
assert (BASE/"compile-time.edn").read_text()=="[true true nil]"
files=["crates/suss-compile/src/portable_core_macros.rs","crates/suss-compile/src/portable_macros.rs","crates/suss-cli/tests/clojure_assert.rs","docs/compatibility/bootstrap/exact-integers.sus","docs/compatibility/core-import.json","runtime/core-import/suss/core.sus","runtime/core-import/manifest.json","results/m12-assert/upstream-assert.clj","results/m12-assert/assert-adapted.sus","results/m12-assert/assert.patch","results/m12-assert/cases.json","results/m12-assert/command.json","results/m12-assert/oracle.clj"]
record={"primary":{"upstream":recorded["upstream"],"jar_sha256":recorded["jars"][str(jar)],"entry":recorded["entry"],"entry_sha256":recorded["entry_sha256"],"original_form_sha256":sha(original)},"license":{"id":"EPL-1.0","notice":"results/m12-core-dependencies/NOTICE","text":"runtime/core-import/epl-v10.html"},"adaptation":"Retain complete macro/doc/metadata/arities/control flow/message expressions. Replace only two JVM AssertionError constructor calls with the existing catchable portable Error adapter, as other compiled core macros do. The Macro phase reads its own dynamic *assert* Var before operand analysis; default true.","limits":"Java AssertionError class identity/metadata is not provided by the existing portable Error family and is not claimed here. Primary observations cover control flow, messages, fresh error identity, ex-data, lexical shadowing and compile-time flags; not JVM class interoperability. Native execution/body acceptance requires separate receipts.","primary_controls":19,"compile_time_primary":[True,True,None],"files":{p:sha((ROOT/p).read_bytes()) for p in files}}
path=ROOT/"docs/compatibility/bootstrap/clojure-assert.json"
if "--record" in sys.argv:path.write_text(json.dumps(record,indent=2)+"\n")
else:assert json.loads(path.read_text())==record
print("Complete assert source/adaptation/primary "+("recorded" if "--record" in sys.argv else "verified"))
