"""Check actual upstream Var adaptation and fresh pinned primary observations."""
from pathlib import Path
import hashlib,json,sys
ROOT=Path(__file__).resolve().parents[2];BASE=Path(__file__).resolve().parent
sha=lambda raw:hashlib.sha256(raw).hexdigest()
manifest=json.loads((ROOT/'runtime/core-import/manifest.json').read_text())
entry=next(row for row in manifest['forms'] if row['id']=='runtime:Var:1187')
raw=(ROOT/entry['source']).read_bytes()
original=raw[entry['byte-start']:entry['byte-end']]
assert sha(raw)==entry['source-file-sha256']
assert sha(original)==entry['source-sha256']
patch=json.loads((ROOT/entry['patch']).read_text())
assert patch['source-sha256']==sha(original)
adapted=patch['replacement'].encode()
assert sha(adapted)==entry['adapted-form-sha256']
assert adapted in (ROOT/'runtime/core-import/suss/core.sus').read_bytes()
assert adapted.count(b'(-invoke [')==original.count(b'(-invoke [')==22
assert b'\n  Fn\n' not in adapted and b'\n  IWithMeta\n' not in adapted
p=BASE/'live-vars-primary';command=json.loads((p/'command.json').read_text())
assert command['exit_code']==0 and command['controls']==10
assert all(sha(Path(name).read_bytes())==digest for name,digest in command['jars'].items())
assert sha((p/'oracle.clj').read_bytes())==command['source_sha256']
assert sha((p/'stdout.log').read_bytes())==command['stdout_sha256']
rows=json.loads((p/'cases.json').read_text());assert len(rows)==10
assert [row['expected'] for row in rows]==[json.loads(line) for line in (p/'stdout.log').read_text().splitlines()]
paths=['crates/suss-compile/src/portable/hir/vars.rs','crates/suss-compile/src/portable/hir.rs','crates/suss-compile/src/portable/ir.rs','crates/suss-compile/src/portable/resolve.rs','crates/suss-compile/src/portable_session/evaluation.rs','crates/suss-compile/src/portable_macros.rs','crates/suss-compile/src/portable_session.rs','docs/compatibility/bootstrap/exact-integers.sus','docs/compatibility/patches/expression-Var.json','docs/compatibility/core-import.json','runtime/core-import/manifest.json','runtime/core-import/suss/core.sus','crates/suss-cli/tests/clojure_eval.rs','crates/suss-compile/tests/support/pinned_eval_body.rs','results/m12-eval/live-vars-primary/cases.json','results/m12-eval/live-vars-primary/setup.sus']
paths += ['crates/suss-compile/src/portable/emit.rs','crates/suss-compile/src/runtime_abi.rs','crates/suss-compile/src/portable/origin.rs','crates/suss-compile/src/portable_macro_data.rs','crates/suss-compile/src/portable_macros/definitions.rs','crates/suss-compile/src/portable_macros/native_invocation.rs','crates/suss-compile/src/portable_macros/evaluation.rs','crates/suss-compile/src/portable_macros/facade.rs','crates/suss-compile/src/portable_session/evaluation/macro_function.rs','crates/suss-reader/src/forms.rs','crates/suss-reader/src/edn/source.rs','crates/suss-compile/tests/runtime_evaluation.rs','crates/suss-compile/tests/portable_symbol_conversion.rs','results/m12-eval/macro-declaration-primary/cases.json','results/m12-eval/var-implementation-primary/cases.json']
paths += ['crates/suss-compile/src/portable_macro_graph.rs','crates/suss-compile/src/portable/mod.rs','crates/suss-compile/src/portable/analysis_record.rs','crates/suss-compile/src/portable/modules.rs','crates/suss-cli/tests/compiled_macro_binding_records.rs','crates/suss-cli/tests/compiled_macro_declaration_functions.rs','crates/suss-cli/tests/compiled_macro_source_environment.rs','results/m12-eval/alias-primary/cases.json','results/m12-eval/macro-environment-primary/cases.json','results/m12-eval/macro-failure-primary/cases.json']
paths += ['docs/compatibility/patches/expression-symbol.json','results/m12-eval/symbol-primary/cases.json']
paths += ['crates/suss-compile/src/portable_session/installation.rs', 'crates/suss-compile/src/portable_session/compiler_state.rs', 'crates/suss-compile/src/portable_session/evaluation/function_pool.rs', 'crates/suss-compile/src/portable_session/evaluation/math_context.rs', 'crates/suss-compile/src/portable_macro_data/exception.rs', 'results/m12-eval/function-def-and-pool-primary/cases.json', 'results/m12-eval/function-pool-initialization-primary/cases.json', 'results/m12-eval/exception-transfer-primary/cases.json', 'results/m12-eval/function-metadata-primary/cases.json', 'results/m12-eval/materialization-and-context-primary/cases.json']
paths += ['crates/suss-compile/src/runtime_abi/dynamic.rs', 'crates/suss-compile/src/runtime_abi/decimals.rs', 'crates/suss-compile/tests/runtime_abi.rs', 'results/m12-eval/lazy-math-context-primary/cases.json', 'results/m12-eval/lazy-math-context-primary/expression.sus']
record={'upstream':{'commit':manifest['upstream-commit'],'entry':entry},'primary':command,'source_files':{path:sha((ROOT/path).read_bytes()) for path in paths},'license':'runtime/core-import/epl-v10.html','adaptation':patch['rationale'],'qualification':'Source/provenance verification only. Native results retain their own frozen-source receipts; this does not establish all eval or numeric issue acceptance.'}
path=BASE/'live-vars-source-provenance.json'
if '--record' in sys.argv:path.write_text(json.dumps(record,indent=2)+'\n')
else:assert json.loads(path.read_text())==record
print('Live Var source/adaptation/primary '+('recorded' if '--record' in sys.argv else 'verified'))
