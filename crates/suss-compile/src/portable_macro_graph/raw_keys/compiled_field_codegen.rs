//! Development generator for immutable original-BYTE field tables. No Session,
//! shipped loader, Graph dispatch or runtime field admission changes here.
//! Generator allocation/formatting is development work, not a runtime saving.
use super::{EntryStats, Interner, Plan};
#[cfg(test)]
use crate::portable::bootstrap;
use crate::portable::{
    Diagnostic,
    resolve::{Environment, Phase},
};
use std::fmt::Write;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::portable_macro_graph) struct NativeKey {
    pub start: usize,
    pub len: usize,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::portable_macro_graph) struct NativeNode {
    pub key: usize,
    pub metadata_count: usize,
    pub children_start: usize,
    pub children_len: usize,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::portable_macro_graph) struct NativeField {
    pub owner: usize,
    pub selector: usize,
    pub root: usize,
    pub nodes_start: usize,
    pub nodes_len: usize,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::portable_macro_graph) struct NativeGlobal {
    pub ordinal: usize,
    pub namespace: &'static str,
    pub name: &'static str,
    pub fields_start: usize,
    pub fields_len: usize,
    pub presence: u8,
}
pub(in crate::portable_macro_graph) struct NativePhase {
    pub phase: Phase,
    pub source_sha256: &'static str,
    pub dependency_sha256: &'static str,
    pub keys: &'static [NativeKey],
    pub key_bytes: &'static [u8],
    pub nodes: &'static [NativeNode],
    pub children: &'static [usize],
    pub fields: &'static [NativeField],
    pub globals: &'static [NativeGlobal],
}
fn error(message: &str) -> Diagnostic {
    Diagnostic {
        span: 0..0,
        message: message.into(),
    }
}
fn plus(a: usize, b: usize) -> Result<usize, Diagnostic> {
    a.checked_add(b)
        .ok_or_else(|| error("Native field table extent overflow"))
}
struct GlobalOutput {
    ordinal: usize,
    namespace: String,
    name: String,
    fields_start: usize,
    fields_len: usize,
    presence: u8,
}
struct Output {
    keys: Vec<NativeKey>,
    bytes: Vec<u8>,
    nodes: Vec<NativeNode>,
    children: Vec<usize>,
    fields: Vec<NativeField>,
    globals: Vec<GlobalOutput>,
    planner_work: u64,
}
fn field<'a>(
    info: &'a crate::portable::resolve::DefinitionInfo,
    selector: usize,
) -> Option<&'a suss_reader::forms::Form> {
    match selector {
        0 => Some(&info.definition_form),
        1 => Some(&info.declaration),
        2 => info.initializer_form.as_ref(),
        _ => None,
    }
}
fn collect(phase: Phase, env: &Environment) -> Result<Output, Diagnostic> {
    let scope = env
        .namespace_scope_for(phase, "suss.core")
        .ok_or_else(|| error("Missing actual core catalog"))?;
    let mut out = Output {
        keys: Vec::new(),
        bytes: Vec::new(),
        nodes: Vec::new(),
        children: Vec::new(),
        fields: Vec::new(),
        globals: Vec::new(),
        planner_work: 0,
    };
    let mut table = Interner::default();
    let mut plan = Plan::default();
    let mut work = 0u64;
    let mut charge = |units: u64| {
        work = work
            .checked_add(units)
            .ok_or_else(|| error("Native field generator work overflow"))?;
        Ok::<(), Diagnostic>(())
    };
    for (ordinal, (global, info)) in scope.declarations.iter().enumerate() {
        if !info.analysis_completed {
            continue;
        }
        let actual = env
            .shared_definition_info(global)
            .ok_or_else(|| error("Missing actual shared definition"))?;
        if global.phase() != phase || !std::ptr::eq(*info, actual.as_ref()) {
            return Err(error("Native field owner correspondence"));
        }
        let owner = out.globals.len();
        let fields_start = out.fields.len();
        for selector in 0..3 {
            if let Some(form) = field(actual, selector) {
                let root = table.plan(form, &mut plan, &mut charge)?;
                let nodes_start = out.nodes.len();
                plus(nodes_start, plan.nodes.len())?;
                for node in &plan.nodes {
                    let children_start = out.children.len();
                    plus(children_start, node.children.len())?;
                    for &child in &node.children {
                        if child >= plan.nodes.len() {
                            return Err(error("Original field child outside LOCAL plan"));
                        }
                        out.children.push(child); // intentionally LOCAL, never offset by nodes_start
                    }
                    out.nodes.push(NativeNode {
                        key: node.key.0,
                        metadata_count: node.metadata_count,
                        children_start,
                        children_len: node.children.len(),
                    });
                }
                out.fields.push(NativeField {
                    owner,
                    selector,
                    root,
                    nodes_start,
                    nodes_len: plan.nodes.len(),
                });
                plan.clear_metered(&mut charge)?;
            }
        }
        out.globals.push(GlobalOutput {
            ordinal,
            namespace: global.namespace().into(),
            name: global.name().into(),
            fields_start,
            fields_len: out.fields.len() - fields_start,
            presence: if actual.initializer_form.is_some() {
                7
            } else {
                3
            },
        });
    }
    let mut stats = EntryStats::default();
    for index in 0..table.keys.len() {
        let entry = table.keys.get(index, &mut charge, &mut stats)?;
        let start = out.bytes.len();
        plus(start, entry.key.len())?;
        out.bytes.extend_from_slice(&entry.key);
        out.keys.push(NativeKey {
            start,
            len: entry.key.len(),
        });
    }
    out.planner_work = work;
    Ok(out)
}
// Generated source is deterministic. No compiler fingerprint is emitted: that
// fingerprint INCLUDES this generated file and would otherwise self-reference.
const HEADER: &str = "// @generated by suss-native-fields. Do not edit.\n// Original canonical BYTE identities; LOCAL field child indexes. No source capability.\nuse super::raw_keys::compiled_field_codegen::{NativeKey,NativeNode,NativeField,NativeGlobal,NativePhase};\nuse crate::portable::resolve::Phase;\nconst fn k(start:usize,len:usize)->NativeKey{NativeKey{start,len}}\nconst fn n(key:usize,metadata_count:usize,children_start:usize,children_len:usize)->NativeNode{NativeNode{key,metadata_count,children_start,children_len}}\nconst fn f(owner:usize,selector:usize,root:usize,nodes_start:usize,nodes_len:usize)->NativeField{NativeField{owner,selector,root,nodes_start,nodes_len}}\n";
fn emit(
    out: &mut String,
    name: &str,
    phase: Phase,
    data: &Output,
    source: &str,
    dependencies: &str,
) {
    writeln!(out,"pub(super) static {name}:NativePhase=NativePhase{{phase:Phase::{phase:?},source_sha256:{source:?},dependency_sha256:{dependencies:?},").unwrap();
    writeln!(out, "keys:&[").unwrap();
    for key in &data.keys {
        writeln!(out, "k({},{}),", key.start, key.len).unwrap();
    }
    writeln!(out, "],key_bytes:&[").unwrap();
    for chunk in data.bytes.chunks(32) {
        for byte in chunk {
            write!(out, "{byte},").unwrap();
        }
        out.push('\n');
    }
    writeln!(out, "],nodes:&[").unwrap();
    for node in &data.nodes {
        writeln!(
            out,
            "n({},{},{},{}),",
            node.key, node.metadata_count, node.children_start, node.children_len
        )
        .unwrap();
    }
    writeln!(out, "],children:&[").unwrap();
    for chunk in data.children.chunks(32) {
        for index in chunk {
            write!(out, "{index},").unwrap();
        }
        out.push('\n');
    }
    writeln!(out, "],fields:&[").unwrap();
    for field in &data.fields {
        writeln!(
            out,
            "f({},{},{},{},{}),",
            field.owner, field.selector, field.root, field.nodes_start, field.nodes_len
        )
        .unwrap();
    }
    writeln!(out, "],globals:&[").unwrap();
    for g in &data.globals {
        writeln!(out,"NativeGlobal{{ordinal:{},namespace:{:?},name:{:?},fields_start:{},fields_len:{},presence:{}}},",g.ordinal,g.namespace,g.name,g.fields_start,g.fields_len,g.presence).unwrap();
    }
    writeln!(out, "]}};\n").unwrap();
}
pub(in crate::portable_macro_graph) fn generate(
    mut catalog: impl FnMut(Phase) -> Result<Environment, Diagnostic>,
    source: &str,
    dependencies: &str,
) -> Result<String, Diagnostic> {
    let mut output = String::from(HEADER);
    for (phase, name) in [(Phase::Runtime, "RUNTIME"), (Phase::Macro, "MACRO")] {
        let env = catalog(phase)?;
        let data = collect(phase, &env)?;
        // No hard user form cap. These are observed generator counts, not a
        // quota or a way to truncate an expanded catalog into the old fixture.
        eprintln!(
            "native-field-codegen phase={phase:?} owners={} fields={} keys={} nodes={} children={} canonical_bytes={} original_plan_and_key_snapshot_work={} development_only=true runtime_admission=false",
            data.globals.len(),
            data.fields.len(),
            data.keys.len(),
            data.nodes.len(),
            data.children.len(),
            data.bytes.len(),
            data.planner_work
        );
        emit(&mut output, name, phase, &data, source, dependencies);
    }
    Ok(output)
}
#[cfg(test)]
mod tests {
    use super::super::super::generated_native_fields;
    use super::*;
    fn selected(phase: Phase) -> &'static NativePhase {
        match phase {
            Phase::Runtime => &generated_native_fields::RUNTIME,
            Phase::Macro => &generated_native_fields::MACRO,
        }
    }
    #[test]
    fn compiled_tables_match_all_actual_original_byte_keys_fields_and_local_plan_rows() {
        for phase in [Phase::Runtime, Phase::Macro] {
            let data = selected(phase);
            assert_eq!(data.phase, phase);
            assert_eq!(
                data.source_sha256,
                bootstrap::sha256(bootstrap::SOURCE.as_bytes()),
                "Run generator first; scaffold is not evidence"
            );
            assert_eq!(
                data.dependency_sha256,
                bootstrap::sha256(include_bytes!(
                    "../../../../../runtime/core-import/manifest.json"
                ))
            );
            let prepared = bootstrap::shipped(phase).unwrap();
            let scope = prepared
                .environment
                .namespace_scope_for(phase, "suss.core")
                .unwrap();
            let mut table = Interner::default();
            let mut plan = Plan::default();
            let mut work = 0u64;
            let mut charge = |n: u64| {
                work = work.checked_add(n).unwrap();
                Ok::<(), Diagnostic>(())
            };
            let (mut owner_index, mut field_index, mut node_index, mut child_index) =
                (0usize, 0usize, 0usize, 0usize);
            for (ordinal, (global, info)) in scope.declarations.iter().enumerate() {
                if !info.analysis_completed {
                    continue;
                }
                let actual = prepared.environment.shared_definition_info(global).unwrap();
                assert!(std::ptr::eq(*info, actual.as_ref()));
                let owner = &data.globals[owner_index];
                assert_eq!(
                    (owner.ordinal, owner.namespace, owner.name),
                    (ordinal, global.namespace(), global.name())
                );
                assert_eq!(owner.fields_start, field_index);
                assert_eq!(
                    owner.presence,
                    if actual.initializer_form.is_some() {
                        7
                    } else {
                        3
                    }
                );
                let first_field = field_index;
                for selector in 0..3 {
                    if let Some(form) = field(actual, selector) {
                        let root = table.plan(form, &mut plan, &mut charge).unwrap();
                        let expected = &data.fields[field_index];
                        assert_eq!(
                            (
                                expected.owner,
                                expected.selector,
                                expected.root,
                                expected.nodes_start,
                                expected.nodes_len
                            ),
                            (owner_index, selector, root, node_index, plan.nodes.len())
                        );
                        assert!(root < expected.nodes_len);
                        for row in &plan.nodes {
                            let node = &data.nodes[node_index];
                            assert_eq!(
                                (
                                    node.key,
                                    node.metadata_count,
                                    node.children_start,
                                    node.children_len
                                ),
                                (
                                    row.key.0,
                                    row.metadata_count,
                                    child_index,
                                    row.children.len()
                                )
                            );
                            let end = child_index.checked_add(node.children_len).unwrap();
                            assert_eq!(
                                &data.children[child_index..end],
                                row.children.as_slice(),
                                "ALL local child order/occurrences, not global offsets"
                            );
                            for child in &data.children[child_index..end] {
                                assert!(*child < expected.nodes_len);
                            }
                            child_index = end;
                            node_index += 1;
                        }
                        plan.clear_metered(&mut charge).unwrap();
                        field_index += 1;
                    }
                }
                assert_eq!(owner.fields_len, field_index - first_field);
                owner_index += 1;
            }
            assert_eq!(owner_index, 615);
            assert_eq!(field_index, 1577);
            assert_eq!(
                (owner_index, field_index, node_index, child_index),
                (
                    data.globals.len(),
                    data.fields.len(),
                    data.nodes.len(),
                    data.children.len()
                )
            );
            assert_eq!(table.keys.len(), data.keys.len());
            let mut stats = EntryStats::default();
            let mut byte_index = 0;
            for (id, key) in data.keys.iter().enumerate() {
                assert_eq!(key.start, byte_index);
                let end = key.start.checked_add(key.len).unwrap();
                let entry = table.keys.get(id, &mut charge, &mut stats).unwrap();
                assert_eq!(
                    &data.key_bytes[key.start..end],
                    entry.key.as_slice(),
                    "EXACT canonical BYTE key ID{id}"
                );
                byte_index = end;
            }
            assert_eq!(byte_index, data.key_bytes.len());
            println!(
                "compiled-native-fields phase={phase:?} owners={owner_index} fields={field_index} nodes={node_index} children={child_index} keys={} bytes={byte_index} fresh_original_work={} source_binding=UNIMPLEMENTED runtime_admission=UNIMPLEMENTED acceptance=false",
                data.keys.len(),
                work
            );
        }
    }
    #[test]
    fn generated_native_tables_reproduce_exact_source_without_compiler_fingerprint_cycle() {
        let fresh = bootstrap::generate_native_field_tables().unwrap();
        assert_eq!(
            fresh.as_bytes(),
            include_bytes!("../generated_native_fields.rs")
        );
        // Complete typed-array equivalence is checked separately above, not
        // inferred from this reproducibility/text assertion.
    }
}
