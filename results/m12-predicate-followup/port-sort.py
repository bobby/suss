from pathlib import Path
import hashlib,json,sys,zipfile
ROOT=Path(__file__).resolve().parents[2];OUT=Path(__file__).resolve().parent
sys.path.insert(0,str(ROOT/'scripts'));import core_import
# Share the existing precise Number class diagnostics without changing the
# MathContext constructor's null rule or its remaining coercion stages.
f=ROOT/'crates/suss-compile/src/runtime_abi/decimals/context/constructor.rs';s=f.read_text();a=s.index('fn class_cast(');b=s.index('pub(super) fn function(',a)
shared=s[a:b].replace('fn nonnumber_errors(code: &mut Vec<Instruction<\'static>>) {','pub(super) fn nonnumber_errors(code: &mut Vec<Instruction<\'static>>, nil_message: &str) {')
old='''    code.extend(error(
        "Cannot invoke \\\"java.lang.Character.charValue()\\\" because \\\"x\\\" is null",
    ));'''
assert old in shared
shared=shared.replace(old,'    code.extend(error(nil_message));')
header='''//! Shared native Number class diagnostics for constructors and intValue.
use super::*;
use Instruction::*;
fn error(message: &str) -> Vec<Instruction<'static>> {
    numeric::error(message, nominal::ERROR_GLOBAL)
}
'''
(ROOT/'crates/suss-compile/src/runtime_abi/numeric_cast_errors.rs').write_text(header+shared)
s=s[:a]+s[b:];assert s.count('nonnumber_errors(&mut code);')==1
s=s.replace('nonnumber_errors(&mut code);','crate::runtime_abi::numeric_cast_errors::nonnumber_errors(&mut code,\n        "Cannot invoke \\\"java.lang.Character.charValue()\\\" because \\\"x\\\" is null");')
f.write_text(s)
f=ROOT/'crates/suss-compile/src/runtime_abi.rs';s=f.read_text();assert s.count('mod numeric;')==1;f.write_text(s.replace('mod numeric;','mod numeric;\nmod numeric_cast_errors;'))
f=ROOT/'crates/suss-compile/src/runtime_abi/bitwise.rs';s=f.read_text();needle='    let mut dispatch = vec![LocalGet(1), Call(b.names["long-test"]), I32Eqz, If(BlockType::Empty)];';assert s.count(needle)==1
addition='''    // AFunction.compare uses Number.intValue, rather than a sign test or
    // checked RT.intCast. Keep canonical class/null diagnostics before the
    // existing exact unchecked numeric conversion; it does not consult context.
    let mut int_value = Vec::new();
    numeric_cast_errors::nonnumber_errors(&mut int_value,
        "Cannot invoke \\\"java.lang.Number.intValue()\\\" because \\\"n\\\" is null");
    int_value.extend([LocalGet(0), Call(b.names["number-cast-unchecked-int-raw"]),
        Call(b.names["long-box"])]);
    b.function("number-int-value", &[VALUE], &[VALUE], &int_value);
'''
f.write_text(s.replace(needle,addition+needle))
f=ROOT/'crates/suss-compile/src/portable/hir.rs';s=f.read_text();assert s.count('    Cast,')==1;s=s.replace('    Cast,','    Cast,\n    /// Private Java Number.intValue conversion for Clojure comparators.\n    IntValue,')
s=s.replace('            Self::Cast => "number-cast",','            Self::Cast => "number-cast",\n            Self::IntValue => "number-int-value",')
s=s.replace('            | Self::ToLong\n','            | Self::ToLong\n            | Self::IntValue\n')
s=s.replace('                "cast" => Some(Bitwise::Cast),','                "cast" => Some(Bitwise::Cast),\n                "int-value" => Some(Bitwise::IntValue),');f.write_text(s)
f=ROOT/'crates/suss-compile/src/portable/emit.rs';s=f.read_text();needle='| "number-to-long" | "number-hash"';assert s.count(needle)==1;f.write_text(s.replace(needle,'| "number-to-long" | "number-int-value" | "number-hash"'))
# Licensed pinned declaration and explicit original stable WasmGC algorithm.
source=(ROOT/'clojurescript/src/main/cljs/cljs/core.cljs').read_text();forms=core_import.extract_forms(source,'runtime','clojurescript/src/main/cljs/cljs/core.cljs');form=forms['runtime:sort:2557'];digest=hashlib.sha256(form['form'].encode()).hexdigest();path='docs/compatibility/patches/core-stable-sort.json'
rationale="Retain the complete pinned sort docstring, both arities, default live compare/sort reads, eager to-array staging, sequence result and metadata handling. Replace Closure stableSort with an original bottom-up stable merge sort over fixed WasmGC object arrays. Preserve the JVM's object-array mutation and primitive-array copy distinction. Comparator calls follow actual Clojure1.12.6 AFunction.compare: Boolean result branches/reverse truthiness, otherwise Number.intValue with exact unchecked low-bit/truncation conversion and canonical class/null errors. No Java code is shipped. Runtime/Macro/actual AOT regression controls come from the pinned JVM, not JavaScript recordings."
patch={'schema':1,'source-sha256':digest,'replacement':(OUT/'sort-body.sus').read_text().strip(),'rationale':rationale};(ROOT/path).write_text(json.dumps(patch,indent=2)+'\n')
f=ROOT/'docs/compatibility/core-import.json';r=json.loads(f.read_text());assert all(x['id']!=form['id'] for x in r['forms']);r['forms'].append({'id':form['id'],'source-sha256':digest,'patch':path});f.write_text(json.dumps(r,indent=2)+'\n')
f=ROOT/'docs/compatibility/reviews.edn';s=f.read_text();needle=' :reviews {\n';assert s.count(needle)==1
entry=f'''  "runtime:sort:2557"
  {{:source-sha256 "{digest}"
   :visibility :public :arities {{:fixed [1 2] :variadic-min nil}}
   :dependencies ["runtime:compare" "runtime:seq:1282" "runtime:to-array:3819" "runtime:with-meta" "runtime:meta" "suss.bootstrap/object-array?" "suss.bootstrap/packed-array" "suss.bootstrap/int-value"]
   :classification :adapted :rationale {json.dumps(rationale)}
   :status :in-progress :adaptation-path "{path}" :alternative nil
   :tests ["crates/suss-cli/tests/clojure_stable_sort.rs"]}}
'''
f.write_text(s.replace(needle,needle+entry,1))
prior=json.loads((ROOT/'results/m12-wide-wit/oracle-command.json').read_text());jar=next(iter(prior['jars']));raw=zipfile.ZipFile(jar).read('clojure/core.clj');clj=raw.decode();a=clj.index('(defn sort\n');b=clj.index('\n(defn sort-by',a);actual=clj[a:b].strip()
(OUT/'sort-source-provenance.json').write_text(json.dumps({'upstream':'org.clojure/clojure 1.12.6','jar_sha256':prior['jars'][jar],'source':'clojure/core.clj','source_sha256':hashlib.sha256(raw).hexdigest(),'form':'sort','form_sha256':hashlib.sha256(actual.encode()).hexdigest(),'upstream_form':actual,'license':'EPL-1.0','license_path':'runtime/core-import/LICENSE','reference_bytecode':'AFunction-bytecode.log','reference_bytecode_sha256':hashlib.sha256((OUT/'AFunction-bytecode.log').read_bytes()).hexdigest(),'implementation_reference_id':form['id'],'implementation_source_sha256':digest,'adaptation':path,'algorithm':'Original bottom-up stable merge sort, compiled by the existing Macro/Runtime pipeline. No host/JVM sorter is used in shipped code.'},indent=2)+'\n')
