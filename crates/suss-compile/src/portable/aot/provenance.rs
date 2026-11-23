//! Lossless input provenance for an assembled component. Standalone source
//! artifacts retain their complete format2 identity. After verifying inputs,
//! assembly moves those identities here and interns identical macro graphs.
//! These are input manifests, not signatures of Binaryen's rewritten output.
use super::artifact_identity::Manifest;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

pub(super) const SECTION: &str = "suss.input-source-artifacts";
const FORMAT: u32 = 1;
const MAX_BYTES: usize = 32 * 1024 * 1024;
const MAX_EXPANDED: usize = 64 * 1024 * 1024;
const FIELDS: usize = 13;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
struct Index {
    format_version: u32,
    macro_graphs: Vec<Vec<(String, String)>>,
    // All13 original fields. Only macro-dependencies is encoded as nil for
    // unknown or an index for a known graph (including a known empty graph).
    inputs: Vec<Value>,
}

pub(super) fn encode(manifests: &[Manifest]) -> Result<Vec<u8>, String> {
    let mut graphs = Vec::new();
    let mut interned = BTreeMap::new();
    let mut inputs = Vec::with_capacity(manifests.len());
    for manifest in manifests {
        let graph = match &manifest.macro_dependencies {
            None => Value::Null,
            Some(graph) => {
                let index = *interned.entry(graph).or_insert_with(|| {
                    let index = graphs.len();
                    graphs.push(graph.clone());
                    index
                });
                Value::from(index)
            }
        };
        let mut value = serde_json::to_value(manifest).map_err(|error| error.to_string())?;
        let fields = value.as_object_mut().ok_or("Invalid source manifest")?;
        if fields.len() != FIELDS || !fields.contains_key("macro-dependencies") {
            return Err(
                "Source manifest schema changed; update component provenance format".into(),
            );
        }
        fields.insert("macro-dependencies".into(), graph);
        inputs.push(value);
    }
    let index = Index {
        format_version: FORMAT,
        macro_graphs: graphs,
        inputs,
    };
    // Apply the same expansion budget before serialization as the reader does.
    validate_expansion(&index, MAX_EXPANDED)?;
    let bytes = suss_reader::edn::to_string(&index)
        .map_err(|error| error.to_string())?
        .into_bytes();
    if bytes.len() > MAX_BYTES {
        return Err("Component source provenance exceeds bounds".into());
    }
    Ok(bytes)
}

fn validate_expansion(index: &Index, limit: usize) -> Result<(), String> {
    if index.format_version != FORMAT {
        return Err("Unsupported component source provenance format".into());
    }
    let graph_sizes = index
        .macro_graphs
        .iter()
        .map(|graph| {
            serde_json::to_vec(graph)
                .map(|bytes| bytes.len())
                .map_err(|error| error.to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut bytes = 0usize;
    for value in &index.inputs {
        let fields = value
            .as_object()
            .ok_or("Malformed component source manifest")?;
        if fields.len() != FIELDS {
            return Err("Incomplete component source manifest".into());
        }
        let graph = fields
            .get("macro-dependencies")
            .ok_or("Missing component macro provenance")?;
        let graph_bytes = if graph.is_null() {
            0
        } else {
            let id = graph
                .as_u64()
                .and_then(|id| usize::try_from(id).ok())
                .ok_or("Malformed component macro graph reference")?;
            *graph_sizes
                .get(id)
                .ok_or("Component macro graph reference out of range")?
        };
        let record_bytes = serde_json::to_vec(value)
            .map_err(|error| error.to_string())?
            .len();
        bytes = bytes
            .checked_add(record_bytes)
            .and_then(|n| n.checked_add(graph_bytes))
            .ok_or("Component source provenance expansion exceeds bounds")?;
        if bytes > limit {
            return Err("Component source provenance expansion exceeds bounds".into());
        }
    }
    Ok(())
}

fn decode_with_limit(bytes: &[u8], limit: usize) -> Result<Vec<Manifest>, String> {
    if bytes.len() > MAX_BYTES {
        return Err("Component source provenance exceeds bounds".into());
    }
    let text = std::str::from_utf8(bytes).map_err(|error| error.to_string())?;
    let mut index: Index = suss_reader::edn::from_str(text).map_err(|error| error.to_string())?;
    validate_expansion(&index, limit)?;
    let mut manifests = Vec::with_capacity(index.inputs.len());
    for value in &mut index.inputs {
        let fields = value
            .as_object_mut()
            .ok_or("Malformed component source manifest")?;
        let reference = fields
            .get_mut("macro-dependencies")
            .ok_or("Missing component macro provenance")?;
        if !reference.is_null() {
            let id = reference
                .as_u64()
                .and_then(|id| usize::try_from(id).ok())
                .ok_or("Malformed component macro graph reference")?;
            let graph = index
                .macro_graphs
                .get(id)
                .ok_or("Component macro graph reference out of range")?;
            *reference = serde_json::to_value(graph).map_err(|error| error.to_string())?;
        }
        manifests.push(serde_json::from_value(value.take()).map_err(|error| error.to_string())?);
    }
    Ok(manifests)
}

pub(super) fn read(component: &[u8]) -> Result<Vec<Manifest>, String> {
    let mut depth = 0;
    let mut found = None;
    for payload in wasmparser::Parser::new(0).parse_all(component) {
        match payload.map_err(|error| error.to_string())? {
            wasmparser::Payload::Version { encoding, .. } => {
                if depth == 0 && encoding != wasmparser::Encoding::Component {
                    return Err("Input source provenance requires a component".into());
                }
                depth += 1;
            }
            wasmparser::Payload::End(_) => {
                depth -= 1;
            }
            wasmparser::Payload::CustomSection(section)
                if depth == 1 && section.name() == SECTION =>
            {
                if found.is_some() {
                    return Err("Duplicate component source provenance".into());
                }
                found = Some(decode_with_limit(section.data(), MAX_EXPANDED)?);
            }
            _ => {}
        }
    }
    found.ok_or_else(|| "Missing component source provenance".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn manifest(graph: Option<Vec<(String, String)>>) -> Manifest {
        let source = crate::portable::compile("42").unwrap();
        let mut manifest =
            crate::portable::artifact_identity::verify(&source, Default::default()).unwrap();
        manifest.macro_dependencies = graph;
        manifest
    }
    #[test]
    fn sharing_preserves_every_manifest_field_unknown_and_known_empty_graphs() {
        let graph = vec![("macro:λ/case".into(), "a".repeat(64))];
        let inputs = vec![
            manifest(None),
            manifest(Some(vec![])),
            manifest(Some(graph.clone())),
            manifest(Some(graph)),
        ];
        let bytes = encode(&inputs).unwrap();
        let wire: Index = suss_reader::edn::from_str(std::str::from_utf8(&bytes).unwrap()).unwrap();
        assert_eq!(wire.macro_graphs.len(), 2);
        assert_eq!(decode_with_limit(&bytes, MAX_EXPANDED).unwrap(), inputs);
        assert!(
            decode_with_limit(&bytes, 1)
                .unwrap_err()
                .contains("expansion exceeds")
        );
    }
    #[test]
    fn malformed_references_and_incomplete_records_fail_closed() {
        let mut wire = Index {
            format_version: FORMAT,
            macro_graphs: vec![],
            inputs: vec![serde_json::to_value(manifest(None)).unwrap()],
        };
        wire.inputs[0]["macro-dependencies"] = Value::from(0);
        let bytes = suss_reader::edn::to_string(&wire).unwrap();
        assert!(
            decode_with_limit(bytes.as_bytes(), MAX_EXPANDED)
                .unwrap_err()
                .contains("out of range")
        );
        wire.inputs[0]
            .as_object_mut()
            .unwrap()
            .remove("macro-dependencies");
        let bytes = suss_reader::edn::to_string(&wire).unwrap();
        assert!(decode_with_limit(bytes.as_bytes(), MAX_EXPANDED).is_err());
    }
}
