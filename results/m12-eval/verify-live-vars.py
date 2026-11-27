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
paths=['crates/suss-compile/src/portable/hir/vars.rs','crates/suss-compile/src/portable/hir.rs','crates/suss-compile/src/portable/ir.rs','crates/suss-compile/src/portable/resolve.rs','crates/suss-compile/src/portable_session/evaluation.rs','crates/suss-compile/src/portable_macros.rs','crates/suss-compile/src/portable_session.rs','docs/compatibility/bootstrap/exact-integers.sus','docs/compatibility/patches/expression-Var.json','docs/compatibility/core-import.json','runtime/core-import/manifest.json','runtime/core-import/suss/core.sus','crates/suss-cli/tests/native_host_boundary.rs','results/m12-eval/live-vars-primary/cases.json','results/m12-eval/live-vars-primary/setup.sus']
paths += ['crates/suss-cli/tests/runtime_namespaces.rs']
paths += ['crates/suss-compile/src/portable/emit.rs','crates/suss-compile/src/runtime_abi.rs','crates/suss-compile/src/portable/origin.rs','crates/suss-compile/src/portable_macro_data.rs','crates/suss-compile/src/portable_macros/definitions.rs','crates/suss-compile/src/portable_macros/native_invocation.rs','crates/suss-compile/src/portable_macros/evaluation.rs','crates/suss-compile/src/portable_macros/facade.rs','crates/suss-compile/src/portable_session/evaluation/macro_function.rs','crates/suss-reader/src/forms.rs','crates/suss-reader/src/edn/source.rs','crates/suss-compile/tests/runtime_evaluation.rs','crates/suss-compile/tests/portable_symbol_conversion.rs','results/m12-eval/macro-declaration-primary/cases.json','results/m12-eval/var-implementation-primary/cases.json']
paths += ['crates/suss-compile/src/portable_macro_graph.rs','crates/suss-compile/src/portable/mod.rs','crates/suss-compile/src/portable/analysis_record.rs','crates/suss-compile/src/portable/modules.rs','crates/suss-cli/tests/compiled_macro_binding_records.rs','crates/suss-cli/tests/compiled_macro_declaration_functions.rs','crates/suss-cli/tests/compiled_macro_source_environment.rs','results/m12-eval/alias-primary/cases.json','results/m12-eval/macro-environment-primary/cases.json','results/m12-eval/macro-failure-primary/cases.json']
paths += ['docs/compatibility/patches/expression-symbol.json','results/m12-eval/symbol-primary/cases.json']
paths += ['crates/suss-compile/src/portable_session/installation.rs', 'crates/suss-compile/src/portable_session/compiler_state.rs', 'crates/suss-compile/src/portable_session/evaluation/function_pool.rs', 'crates/suss-compile/src/portable_session/evaluation/math_context.rs', 'crates/suss-compile/src/portable_macro_data/exception.rs', 'results/m12-eval/function-def-and-pool-primary/cases.json', 'results/m12-eval/function-pool-initialization-primary/cases.json', 'results/m12-eval/exception-transfer-primary/cases.json', 'results/m12-eval/function-metadata-primary/cases.json', 'results/m12-eval/materialization-and-context-primary/cases.json']
paths += ['crates/suss-compile/src/runtime_abi/dynamic.rs', 'crates/suss-compile/src/runtime_abi/decimals.rs', 'crates/suss-compile/tests/runtime_abi.rs', 'results/m12-eval/lazy-math-context-primary/cases.json', 'results/m12-eval/lazy-math-context-primary/expression.sus']
p=BASE/'unbound-dynamic-primary';unbound=json.loads((p/'command.json').read_text())
assert unbound['exit_code']==0
assert all(sha(Path(name).read_bytes())==digest for name,digest in unbound['jars'].items())
assert sha((p/'expression.clj').read_bytes())==unbound['source_sha256']
assert sha((p/'primary.stdout').read_bytes())==unbound['stdout_sha256']
rows=json.loads((p/'cases.json').read_text());assert len(rows)==6
assert [row['observed_edn'] for row in rows]==(p/'primary.stdout').read_text().splitlines()
assert (p/'expression.clj').read_text()=='(assert (= "1.12.6" (clojure-version)))\n'+''.join('(prn '+row['expression']+')\n' for row in rows)
assert (p/'expression.sus').read_text()=='['+'\n'.join('(try '+row['expression'].replace('(catch Throwable ', '(catch :default ')+' (catch :default unexpected :unexpected-native-error))' for row in rows)+']\n'
paths += ['crates/suss-cli/tests/compiled_macro_publication_cells.rs']
paths += [str(path.relative_to(ROOT)) for path in sorted(p.iterdir()) if path.is_file()]
paths += ['crates/suss-compile/tests/portable_resolution.rs', 'crates/suss-compile/tests/portable_closures.rs', 'crates/suss-compile/tests/portable_dynamic_bindings.rs']
p=BASE/'binding-initializer-effects-primary';initializer=json.loads((p/'command.json').read_text())
assert initializer['exit_code']==0 and initializer['controls']==9
assert all(sha(Path(name).read_bytes())==digest for name,digest in initializer['jars'].items())
assert sha((p/'expression.clj').read_bytes())==initializer['source_sha256']
assert sha((p/'primary.stdout').read_bytes())==initializer['stdout_sha256']
rows=json.loads((p/'cases.json').read_text());assert len(rows)==9
assert [row['observed_edn'] for row in rows]==(p/'primary.stdout').read_text().splitlines()
assert (p/'expression.clj').read_text()=='(assert (= "1.12.6" (clojure-version)))\n'+''.join('(prn '+row['expression']+')\n' for row in rows)
assert (p/'expression.sus').read_text()=='['+'\n'.join(row['expression'].replace('(catch Throwable ', '(catch :default ') for row in rows)+']\n'
paths += ['crates/suss-compile/tests/runtime_async_scheduler.rs']
paths += [str(path.relative_to(ROOT)) for path in sorted(p.iterdir()) if path.is_file()]
p=BASE/'binding-root-effects-primary';initializer=json.loads((p/'command.json').read_text())
assert initializer['exit_code']==0 and initializer['controls']==8
assert all(sha(Path(name).read_bytes())==digest for name,digest in initializer['jars'].items())
assert sha((p/'expression.clj').read_bytes())==initializer['source_sha256']
assert sha((p/'primary.stdout').read_bytes())==initializer['stdout_sha256']
rows=json.loads((p/'cases.json').read_text());assert len(rows)==8
assert [row['observed_edn'] for row in rows]==(p/'primary.stdout').read_text().splitlines()
assert (p/'expression.clj').read_text()=='(assert (= "1.12.6" (clojure-version)))\n'+''.join('(prn '+row['expression']+')\n' for row in rows)
assert (p/'expression.sus').read_text()=='['+'\n'.join(row['expression'].replace('(catch Throwable ', '(catch :default ') for row in rows)+']\n'
paths += ['crates/suss-compile/tests/runtime_async_scheduler.rs']
paths += [str(path.relative_to(ROOT)) for path in sorted(p.iterdir()) if path.is_file()]
paths += ['crates/suss-cli/tests/portable_dynamic_bindings.rs']
p=BASE/'with-redefs-root-primary';initializer=json.loads((p/'command.json').read_text())
assert initializer['exit_code']==0 and initializer['controls']==12
assert all(sha(Path(name).read_bytes())==digest for name,digest in initializer['jars'].items())
assert sha((p/'expression.clj').read_bytes())==initializer['source_sha256']
assert sha((p/'primary.stdout').read_bytes())==initializer['stdout_sha256']
rows=json.loads((p/'cases.json').read_text());assert len(rows)==12
assert [row['observed_edn'] for row in rows]==(p/'primary.stdout').read_text().splitlines()
assert (p/'expression.clj').read_text()=='(assert (= "1.12.6" (clojure-version)))\n'+''.join('(prn '+row['expression']+')\n' for row in rows)
assert (p/'expression.sus').read_text()=='['+'\n'.join(row['expression'].replace('(catch Throwable ', '(catch :default ') for row in rows)+']\n'
paths += ['crates/suss-compile/tests/runtime_async_scheduler.rs']
paths += [str(path.relative_to(ROOT)) for path in sorted(p.iterdir()) if path.is_file()]
paths += ['crates/suss-cli/tests/portable_dynamic_bindings.rs']
paths += ['crates/suss-compile/src/portable/hir/dynamic.rs','crates/suss-compile/src/runtime_abi/dynamic/redefs.rs','crates/suss-compile/tests/runtime_dynamic_switch.rs','crates/suss-compile/src/portable/analyzer_edn.rs','crates/suss-compile/src/portable/hir/division_overloads.rs','crates/suss-compile/src/portable/hir/namespaces.rs','crates/suss-compile/tests/portable_async_analysis.rs']
p=BASE/'macro-root-and-duplicate-primary'
macro_roots=json.loads((p/'command.json').read_text())
assert macro_roots['exit_code']==0 and macro_roots['controls']==13
assert all(sha(Path(name).read_bytes())==digest for name,digest in macro_roots['jars'].items())
assert sha((p/'expression.clj').read_bytes())==macro_roots['source_sha256']
assert sha((p/'expression.sus').read_bytes())==macro_roots['native_source_sha256']
assert sha((p/'primary.stdout').read_bytes())==macro_roots['stdout_sha256']
assert len(macro_roots['cases'])==13
source='['+'\n'.join(row['expression'] for row in macro_roots['cases'])+']'
assert (p/'expression.clj').read_text()=='(assert (= "1.12.6" (clojure-version)))\n(println (pr-str '+source+'))\n'
assert (p/'expression.sus').read_text()==source.replace('(catch Throwable ', '(catch :default ')+'\n'
paths += [str(path.relative_to(ROOT)) for path in sorted(p.iterdir()) if path.is_file()]
p=BASE/'thread-macro-binding-primary'
thread_macros=json.loads((p/'command.json').read_text())
assert thread_macros['exit_code']==0 and thread_macros['controls']==9
assert all(sha(Path(name).read_bytes())==digest for name,digest in thread_macros['jars'].items())
assert sha((p/'expression.clj').read_bytes())==thread_macros['source_sha256']
assert sha((p/'expression.sus').read_bytes())==thread_macros['native_source_sha256']
assert sha((p/'primary.stdout').read_bytes())==thread_macros['stdout_sha256']
assert len(thread_macros['cases'])==9
source='['+'\n'.join(row['expression'] for row in thread_macros['cases'])+']'
assert (p/'expression.clj').read_text()=='(assert (= "1.12.6" (clojure-version)))\n(prn '+source+')\n'
assert (p/'expression.sus').read_text()==source.replace('(catch Throwable ', '(catch :default ')+'\n'
paths += [str(path.relative_to(ROOT)) for path in sorted(p.iterdir()) if path.is_file()]
p=BASE/'metadata-identity-primary'
identity=json.loads((p/'primary.json').read_text())
assert identity['exit_code']==0
assert all(sha(Path(name).read_bytes())==digest for name,digest in identity['jars'].items())
assert sha((p/'expression.clj').read_bytes())==identity['source_sha256']
assert sha((p/'primary.stdout').read_bytes())==identity['stdout_sha256']
negative=json.loads((p/'native.json').read_text())
assert negative['head']=='8a93677f634c80f4af79fd34be3c7bdbdb55c7dd' and negative['exit_code']==0
assert not negative['matches_primary'] and all(negative[k] for k in ['binary_unchanged','head_unchanged','tracked_source_unchanged'])
paths += [str(path.relative_to(ROOT)) for path in sorted(p.iterdir()) if path.is_file()]
paths += ['crates/suss-compile/src/portable_session/evaluation/macro_binding.rs']
p=BASE/'expanded-macro-collection-primary'
collection=json.loads((p/'command.json').read_text())
assert collection['exit_code']==0 and collection['controls']==4
assert all(sha(Path(name).read_bytes())==digest for name,digest in collection['jars'].items())
assert sha((p/'expression.clj').read_bytes())==collection['source_sha256']
assert sha((p/'expression.sus').read_bytes())==collection['native_source_sha256']
assert sha((p/'primary.stdout').read_bytes())==collection['stdout_sha256']
assert (p/'primary.stdout').read_text().strip()=='[true false false false]'
paths += [str(path.relative_to(ROOT)) for path in sorted(p.iterdir()) if path.is_file()]
paths += ['crates/suss-cli/tests/clojure_chunk_effects.rs','crates/suss-cli/tests/compiled_macro_declaration_functions.rs','crates/suss-cli/tests/core_sorted_interfaces.rs','crates/suss-cli/tests/portable_bitwise_hash.rs','crates/suss-cli/tests/portable_murmur_hash.rs','crates/suss-cli/tests/portable_numeric_hash.rs']
paths += ['crates/suss-cli/tests/compiled_native_entrypoints.rs']
# Public eval has a separate top-level literal boundary. The pinned negative
# nested-Atom cases remain failures; false/nil metadata does not authorize macros.
p=BASE/'self-values-and-false-metadata-primary'
literals=json.loads((p/'command.json').read_text())
assert literals['exit_code']==0 and literals['case_count']==13
assert all(sha(Path(name).read_bytes())==digest for name,digest in literals['jars'].items())
assert sha((p/'expression.clj').read_bytes())==literals['source_sha256']
assert sha((p/'expression.sus').read_bytes())==literals['native_source_sha256']
assert sha((p/'primary.stdout').read_bytes())==literals['stdout_sha256']
rows=json.loads((p/'cases.json').read_text());assert len(rows)==13
source='[\n'+'\n'.join(row['expression'] for row in rows)+'\n]\n'
assert (p/'expression.clj').read_text()=='(prn '+source.strip()+')\n'
assert (p/'expression.sus').read_text()==source.replace('(catch Throwable ', '(catch :default ')
paths += [str(path.relative_to(ROOT)) for path in sorted(p.iterdir()) if path.is_file()]
paths += ['crates/suss-compile/src/runtime_abi/nominal.rs']
paths += [
 'crates/suss-compile/src/portable_session/evaluation/native_values/macro_vars.rs',
 'crates/suss-compile/src/portable_session/evaluation/native_values/function_namespace.rs',
 'crates/suss-compile/src/portable_session/evaluation/native_values/namespace_catalog.rs',
 'crates/suss-compile/src/portable_macro_data/borrowed_reader.rs',
 'crates/suss-compile/src/portable_macros/definitions.rs',
 'crates/suss-compile/src/portable_macros/facade.rs',
 'crates/suss-compile/src/portable/source.rs',
]
# New pinned source-boundary controls retain exact executable inputs and outputs.
for name in ['staged-macro-root-effect-primary', 'staged-macro-root-identity-primary',
             'staged-macro-metadata-root-primary', 'macro-to-ordinary-reload-primary']:
    primary = BASE / name
    receipt = json.loads((primary / 'receipt.json').read_text())
    assert receipt['exit_code'] == 0
    assert all(sha(Path(n).read_bytes()) == h for n, h in receipt['jars'].items())
    assert all(sha((primary / n).read_bytes()) == h for n, h in receipt['files'].items())
    assert any(n.endswith('/clojure/1.12.6/clojure-1.12.6.jar') and h == '21b578db389a8e74b286ce79c4a7f86ec641e26d88b12f18d69030916b0e18ca' for n, h in receipt['jars'].items())
    paths += [str(f.relative_to(ROOT)) for f in sorted(primary.rglob('*')) if f.is_file()]
paths += ['crates/suss-cli/tests/repl_value_display.rs',
          'crates/suss-compile/src/portable/hir/nominal.rs',
          'crates/suss-compile/src/portable/hir/quotes.rs',
          'crates/suss-compile/src/portable_session/evaluation/native_values.rs',
          'crates/suss-compile/tests/portable_calling_convention.rs',
          'crates/suss-compile/tests/portable_comparator_foundations.rs']
primary = BASE / 'declaration-metadata-lexical-primary'
receipt = json.loads((primary / 'receipt.json').read_text())
assert receipt['exit_code'] == 0 and receipt['controls'] == 6
assert all(sha(Path(n).read_bytes()) == h for n, h in receipt['jars'].items())
assert all(sha((primary / n).read_bytes()) == h for n, h in receipt['files'].items())
paths += [str(f.relative_to(ROOT)) for f in sorted(primary.iterdir()) if f.is_file()]
paths += ['crates/suss-cli/tests/compiled_macro_global_reference_asts.rs']
for directory in ['quoted-declaration-primary', 'definition-transition-primary', 'identical-metadata-primary', 'unbound-replacement-primary']:
    primary = ROOT / 'results/native-host-foundation' / directory
    receipt = json.loads((primary / 'receipt.json').read_text())
    assert receipt['exit_code'] == 0
    assert all(sha(Path(n).read_bytes()) == h for n, h in receipt['jars'].items())
    assert all(sha((primary / n).read_bytes()) == h for n, h in receipt['files'].items())
    paths += [str(f.relative_to(ROOT)) for f in sorted(primary.iterdir()) if f.is_file()]
paths += ['crates/suss-cli/tests/compiled_export_metadata.rs', 'crates/suss-cli/tests/compiled_macro_constructor_source_asts.rs']
paths += ['crates/suss-compile/src/runtime_abi/native_values.rs', 'crates/suss-compile/src/runtime_abi/nominal.rs',
          'crates/suss-compile/src/portable_session/evaluation/native_values/operations.rs',
          'crates/suss-compile/src/portable_session/evaluation/native_values/macro_vars.rs',
          'docs/runtime/host-macro-boundary.md']
paths += ['crates/suss-cli/tests/clojure_eval.rs', 'crates/suss-compile/tests/support/pinned_eval_body.rs']
record={'upstream':{'commit':manifest['upstream-commit'],'entry':entry},'primary':command,'source_files':{path:sha((ROOT/path).read_bytes()) for path in paths},'license':'runtime/core-import/epl-v10.html','adaptation':patch['rationale'],'qualification':'Source/provenance verification only. Native results retain their own frozen-source receipts; this does not establish all eval or numeric issue acceptance.'}
path=BASE/'live-vars-source-provenance.json'
if '--record' in sys.argv:path.write_text(json.dumps(record,indent=2)+'\n')
else:assert json.loads(path.read_text())==record
print('Live Var source/adaptation/primary '+('recorded' if '--record' in sys.argv else 'verified'))
